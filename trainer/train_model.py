import os
import glob
import json
import numpy as np
import pandas as pd
import mne
from mne.decoding import CSP
from sklearn.pipeline import Pipeline
from sklearn.discriminant_analysis import LinearDiscriminantAnalysis
from sklearn.model_selection import cross_val_score, StratifiedKFold

# =================配置区=================
# CSV 文件所在的路径 (相对于当前脚本)
DATA_DIR = "../"
# 采样率 (OpenBCI Cyton+Daisy)
SFREQ = 125 
# 这里的标签必须和你录制时输入的 Label 一致
# 0: 基准/放松, 1: 动作/攻击
LABELS_MAP = {
    "Relax": 0,   
    "Attack": 1,  
    # "Walk": 2   # 如果你录了 Walk，可以在这里加
}
# ===========================================

def load_csv_data():
    """
    自动扫描并加载 ../training_data_*.csv
    返回: X (数据矩阵), y (标签列表)
    """
    print(f"🔍 Scanning for data in {os.path.abspath(DATA_DIR)} ...")
    
    # 匹配文件名模式
    csv_files = glob.glob(os.path.join(DATA_DIR, "training_data_*.csv"))
    
    if not csv_files:
        print("❌ Error: No CSV files found!")
        print("   -> Please run 'cargo run', switch to REAL mode, and record some data first.")
        return None, None

    all_epochs = []
    all_labels = []

    for file in csv_files:
        filename = os.path.basename(file)
        
        # 1. 自动识别标签
        label = None
        for name, val in LABELS_MAP.items():
            if name.lower() in filename.lower():
                label = val
                break
        
        if label is None:
            print(f"⚠️ Skipping unknown file: {filename} (Label not in LABELS_MAP)")
            continue

        print(f"   -> Loading: {filename} [Label: {label}]")

        # 2. 读取 CSV
        try:
            df = pd.read_csv(file)
            # 我们的 Rust recorder 输出格式：Timestamp, Ch0, Ch1... Ch15
            # 取第 1 列到第 17 列 (共16通道)
            data = df.iloc[:, 1:17].values.T # 转置为 (n_channels, n_samples)
            
            # 单位转换: 假设 OpenBCI 输出是 uV (微伏), MNE 需要 V (伏特)
            data = data * 1e-6

            # 3. 切片 (Slicing/Epoching)
            # 把长长的一段录音切成无数个 1秒 的小片段用于训练
            n_channels, n_samples = data.shape
            window_size = int(SFREQ * 1.0) # 1秒窗口
            stride = int(SFREQ * 0.5)      # 0.5秒步长 (50% 重叠)

            # 如果数据太短，不够切一片，就跳过
            if n_samples < window_size:
                continue

            for start in range(0, n_samples - window_size + 1, stride):
                end = start + window_size
                segment = data[:, start:end]
                all_epochs.append(segment)
                all_labels.append(label)
                
        except Exception as e:
            print(f"❌ Error reading {filename}: {e}")

    if not all_epochs:
        print("❌ Loaded files but found no valid epochs. Record longer sessions!")
        return None, None

    # 转换为 numpy 数组: (n_epochs, n_channels, n_times)
    X = np.array(all_epochs)
    y = np.array(all_labels)
    
    return X, y

def train_and_export():
    # 1. 准备数据
    X, y = load_csv_data()
    if X is None: return

    print(f"\n📊 Data Summary:")
    print(f"   Total Samples: {X.shape[0]}")
    print(f"   Channels: {X.shape[1]}")
    print(f"   Time Points: {X.shape[2]}")
    print(f"   Class Distribution: {np.bincount(y)}")

    # 2. 定义 AI 模型架构 (CSP + LDA)
    # CSP: 提取脑波的空间特征 (这是处理运动想象的神器)
    csp = CSP(n_components=4, reg=None, log=True, norm_trace=False)
    lda = LinearDiscriminantAnalysis()
    
    pipeline = Pipeline([('CSP', csp), ('LDA', lda)])

    # 3. 评估模型 (Cross-Validation)
    # 看看如果不作弊，模型能打多少分
    cv = StratifiedKFold(n_splits=5, shuffle=True)
    try:
        scores = cross_val_score(pipeline, X, y, cv=cv, scoring='accuracy')
        print(f"\n🏆 Model Accuracy: {np.mean(scores)*100:.2f}% (+/- {np.std(scores)*100:.2f}%)")
    except ValueError:
        print("\n⚠️ Not enough data for Cross-Validation. Training directly...")

    # 4. 全量训练
    print("🚀 Training final model on full dataset...")
    pipeline.fit(X, y)

    # 5. 导出模型参数到 JSON
    # Rust 不需要加载整个 Python 对象，只需要矩阵参数做数学运算即可
    
    # 提取 CSP 滤波器矩阵 (Spatial Filters)
    filters = pipeline.named_steps['CSP'].filters_[:4] # 取前4个分量
    
    # 提取 LDA 权重和截距
    coef = pipeline.named_steps['LDA'].coef_[0]
    intercept = pipeline.named_steps['LDA'].intercept_[0]

    model_data = {
        "version": "1.0",
        "n_channels": 16,
        "csp_filters": filters.tolist(),
        "lda_coef": coef.tolist(),
        "lda_intercept": intercept,
        "classes": list(LABELS_MAP.keys())
    }

    output_file = "../brain_model.json"
    with open(output_file, "w") as f:
        json.dump(model_data, f, indent=4)

    print(f"\n✅ Success! Model saved to: {os.path.abspath(output_file)}")
    print("👉 Now your Rust engine can load this JSON to predict intents!")

if __name__ == "__main__":
    train_and_export()