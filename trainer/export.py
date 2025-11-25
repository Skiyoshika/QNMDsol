import pandas as pd
import matplotlib.pyplot as plt
import glob
import os

def visualize_latest_data():
    data_dir = "../"
    # 找到最新的 CSV 文件
    csv_files = glob.glob(os.path.join(data_dir, "training_data_*.csv"))
    
    if not csv_files:
        print("❌ 没有找到 CSV 数据文件！请先录制。")
        return

    # 按时间排序，取最后一个
    latest_file = max(csv_files, key=os.path.getctime)
    print(f"📈 正在可视化文件: {latest_file}")

    try:
        # 读取数据
        df = pd.read_csv(latest_file)
        
        # 检查数据量
        if df.empty:
            print("⚠️ 文件是空的！")
            return

        # 设置绘图
        plt.figure(figsize=(15, 10))
        plt.suptitle(f"Data Inspection: {os.path.basename(latest_file)}", fontsize=16)

        # 我们只画前 8 个通道 (Ch0 - Ch7)，画太多看不清
        channels_to_plot = 8
        for i in range(channels_to_plot):
            col_name = f"Ch{i}"
            if col_name in df.columns:
                plt.subplot(channels_to_plot, 1, i+1)
                plt.plot(df[col_name], label=col_name, color='C'+str(i), linewidth=0.8)
                plt.legend(loc="upper right")
                plt.ylabel("uV")
                if i == 0:
                    plt.title("Raw EEG Waveforms (First 8 Channels)")
        
        plt.xlabel("Sample Point")
        plt.tight_layout()
        plt.show()
        
        print("✅ 窗口已弹出。如果波形是一条直线，说明接触不良；如果是剧烈波动的正弦波/乱波，说明有信号。")

    except Exception as e:
        print(f"❌ 绘图失败: {e}")

if __name__ == "__main__":
    visualize_latest_data()