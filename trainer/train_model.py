import glob
import json
import os

import mne
import numpy as np
import pandas as pd
from mne.decoding import CSP
from sklearn.discriminant_analysis import LinearDiscriminantAnalysis
from sklearn.model_selection import StratifiedKFold, cross_val_score
from sklearn.pipeline import Pipeline

# ================= 配置区 =================
# CSV 文件所在的路径 (相对于当前脚本)
DATA_DIR = "../"
# 采样率 (转换脚本会重采样到 125 Hz)
SFREQ = 125
# 标签映射：文件名包含 key 即视为该类
LABELS_MAP = {
    "left": 0,   # 左手想象
    "right": 1,  # 右手想象
    "fists": 2,  # 双手想象
    "feet": 3,   # 双脚想象
}
# ========================================


def load_csv_data():
    """
    自动扫描并加载 ../training_data_*.csv
    返回: X (n_epochs, n_channels, n_times), y (labels)
    """
    print(f"Scanning for data in {os.path.abspath(DATA_DIR)} ...")

    csv_files = glob.glob(os.path.join(DATA_DIR, "training_data_*.csv"))
    if not csv_files:
        print("❌ Error: No CSV files found!")
        print("   -> 请先运行数据转换/录制生成 training_data_*.csv")
        return None, None

    all_epochs = []
    all_labels = []
    inferred_channels = None

    for file in csv_files:
        filename = os.path.basename(file)

        # 1. 自动识别标签
        label = None
        for name, val in LABELS_MAP.items():
            if name.lower() in filename.lower():
                label = val
                break

        if label is None:
            print(f"Warning: Skipping unknown file: {filename} (Label not in LABELS_MAP)")
            continue

        print(f"   -> Loading: {filename} [Label: {label}]")

        # 2. 读取 CSV
        try:
            df = pd.read_csv(file)
            # 格式：Timestamp, Ch0, Ch1...
            data = df.iloc[:, 1:].values.T  # (n_channels, n_samples)
            if inferred_channels is None:
                inferred_channels = data.shape[0]

            # 单位从 µV 转 V
            data = data * 1e-6

            # 3. 切片 (1s 窗口，0.5s 步长)
            n_channels, n_samples = data.shape
            window_size = int(SFREQ * 1.0)
            stride = int(SFREQ * 0.5)

            if n_samples < window_size:
                continue

            for start in range(0, n_samples - window_size + 1, stride):
                end = start + window_size
                segment = data[:, start:end]
                all_epochs.append(segment)
                all_labels.append(label)

        except Exception as e:
            print(f"Error reading {filename}: {e}")

    if not all_epochs:
        print("Loaded files but found no valid epochs.")
        return None, None

    X = np.array(all_epochs)
    y = np.array(all_labels)

    print(f"   Inferred channels: {inferred_channels}")
    return X, y


def train_and_export():
    # 1. 准备数据
    X, y = load_csv_data()
    if X is None:
        return

    print(f"\nData Summary:")
    print(f"   Total Samples: {X.shape[0]}")
    print(f"   Channels: {X.shape[1]}")
    print(f"   Time Points: {X.shape[2]}")
    print(f"   Class Distribution: {np.bincount(y)}")

    # 2. 定义模型 (CSP + LDA)
    n_channels = X.shape[1]
    n_components = min(8, max(2, n_channels - 1))
    csp = CSP(n_components=n_components, reg=None, log=True, norm_trace=False)
    lda = LinearDiscriminantAnalysis()
    pipeline = Pipeline([("CSP", csp), ("LDA", lda)])

    # 3. 交叉验证
    cv = StratifiedKFold(n_splits=5, shuffle=True)
    try:
        scores = cross_val_score(pipeline, X, y, cv=cv, scoring="accuracy")
        print(f"\nModel Accuracy: {np.mean(scores)*100:.2f}% (+/- {np.std(scores)*100:.2f}%)")
    except ValueError:
        print("\nNot enough data for Cross-Validation. Training directly...")

    # 4. 全量训练
    print("Training final model on full dataset...")
    pipeline.fit(X, y)

    # 5. 导出模型参数 JSON
    filters = pipeline.named_steps["CSP"].filters_
    lda_step = pipeline.named_steps["LDA"]

    model_data = {
        "version": "1.0",
        "n_channels": int(n_channels),
        "csp_filters": filters.tolist(),
        "lda_coef": lda_step.coef_.tolist(),
        "lda_intercept": lda_step.intercept_.tolist(),
        "classes": lda_step.classes_.tolist(),
    }

    output_file = "../brain_model.json"
    with open(output_file, "w") as f:
        json.dump(model_data, f, indent=4)

    print(f"\nSuccess! Model saved to: {os.path.abspath(output_file)}")
    print("Rust 侧按 classes 顺序解释 LDA 输出即可。")


if __name__ == "__main__":
    train_and_export()
