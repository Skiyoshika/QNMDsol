# Pi 5 调试快速上手（给从没用过树莓派的人）

> 总时长：第一次大约 **1.5 - 2.5 小时**（其中 30-60 分钟是 BrainFlow 在 Pi 上自动编译，不用看着）。
> 你只要会 SSH + 复制粘贴一行命令就能完成。

---

## 0. 你需要这些东西

| 物品 | 备注 |
|---|---|
| Raspberry Pi 5（4G 或 8G 内存都行） | 8G 内存更稳，本项目 4G 也够 |
| **32 GB 或更大** 的 microSD 卡 | A1 速度等级以上，便宜的也行 |
| Pi 5 官方电源（27W USB-C） | 用低瓦电源会随机掉电，不要省 |
| 一根能上网的网线（或者会配 WiFi） | 烧 SD 卡时把 WiFi 密码也设好 |
| SD 卡读卡器（USB） | 插你的笔记本上烧系统用 |
| OpenBCI Cyton+Daisy USB dongle | **后期硬件验证才需要**，前期先不接 |

不需要：HDMI 显示器、键鼠 —— 全程通过 SSH 远程操作。

---

## 1. 烧录系统到 SD 卡（笔记本上做，约 10 分钟）

### 1.1 下载 Imager

去 https://www.raspberrypi.com/software/ 下载 **Raspberry Pi Imager**（Windows / macOS / Linux 都有）。

### 1.2 烧录

打开 Imager：

1. **Choose Device** → 选 **Raspberry Pi 5**
2. **Choose OS** → 选 **Raspberry Pi OS (64-bit)**（默认那个，桌面版或 Lite 都可以，**Lite 更省资源推荐 Lite**）
3. **Choose Storage** → 选你的 SD 卡（**确认不是 U 盘或硬盘**，烧错会丢数据）
4. 点 **NEXT**，**重要：** 弹窗问 "Would you like to apply OS customisation settings?" 选 **EDIT SETTINGS**
   - **Set hostname**: 比如 `neurostick-pi`（这样 SSH 时可以直接 `ssh user@neurostick-pi.local`）
   - **Set username and password**: 自己设（**记下来**）
   - **Configure wireless LAN**: 填 WiFi 名 + 密码（如果用网线则跳过这步也行）
   - 切到 **SERVICES** 标签 → **勾上 Enable SSH**，选 "Use password authentication"
   - **SAVE** → **YES** 开始写入
5. 等 5-10 分钟，写完会自动弹出 SD 卡

### 1.3 上电启动

把 SD 卡插进 Pi 5 → 接网线（或者保证 WiFi 配好了） → 接电源。

**第一次启动比较慢，等 2 分钟。** 看到 Pi 5 上 LED 灯不再频繁闪烁，基本就准备好了。

---

## 2. SSH 进 Pi（笔记本上做）

打开 PowerShell / Terminal：

```bash
ssh 你的用户名@neurostick-pi.local
```

如果 hostname 走 mDNS 不通，用 IP：路由器后台找 Pi 的 IP，或者插显示器开机看一下，然后：

```bash
ssh 你的用户名@192.168.x.x
```

第一次问 "Are you sure you want to continue connecting" → 输入 `yes` → 输入密码 → 登录成功。

> 提示符变成 `用户名@neurostick-pi:~ $` 就成功了。后面所有命令都是在 Pi 上跑，不是在你笔记本上。

---

## 3. 一行命令搞定环境 + 构建 + 自检（在 Pi 上运行）

```bash
curl -fsSL \
  https://raw.githubusercontent.com/Skiyoshika/Neurostick/miki/neurostick-pi5-edge/Neurostick-Pi-5/scripts/pi5-bootstrap.sh \
  | bash
```

这一行脚本会自动做这些事，你只要等：

1. 装 Docker（约 3 分钟）
2. clone 仓库到 `~/Neurostick`
3. **构建 arm64 镜像**（**这步最慢**，**首次 30-60 分钟**，因为要从源码编译 BrainFlow C++ 库）
4. 验证镜像架构 = arm64 ✓
5. 启动 sim 模式容器
6. 调用 `/health` `/status` `/decision` 自检
7. 打印 `SMOKE PASSED.` 表示一切正常

期间你可以喝杯咖啡。**别中断**，中断了下次还得从某个步骤接着来。

### 看到 `SMOKE PASSED.` 之后

恭喜，软件环境全 OK。这时候 Pi 已经能在 sim 模式运行了。验证下：

```bash
# 查看跑着的容器
docker ps

# 看实时 BCI 状态推送（Ctrl+C 退出）
curl -N http://127.0.0.1:8765/events
```

每 ~50ms 会推一行 JSON，里面有 `latest_decision: {best_freq_hz: 12.0, confident: true, ...}`，这就是 sim 模式生成的"假"脑电信号被识别成 12Hz 的结果。**真机用户戴上设备之后，这个 best_freq_hz 就会反映真实的脑电频率响应。**

---

## 4. 接真硬件做完整测试（OpenBCI Cyton+Daisy + dongle）

### 4.1 插 dongle

把 OpenBCI USB dongle 插到 Pi 上。然后：

```bash
# 看 dongle 有没有被识别
ls /dev/serial/by-id/
```

应该能看到一行 `usb-FTDI_FT231X_USB_UART_xxxxxxxx-if00-port0` 之类。**记下这一行的完整路径**。

如果看不到，断电拔下重新插，再 `lsusb` 看下设备列表。

### 4.2 把这个路径喂给 docker compose

```bash
cd ~/Neurostick/Neurostick-Pi-5

# 把识别到的 by-id 路径填进去
export OPENBCI_DEVICE="$(find /dev/serial/by-id -maxdepth 1 -type l | head -n 1)"
echo "用的是这个: $OPENBCI_DEVICE"

# 起服务
docker compose -f docker-compose.pi5.yml up -d
```

### 4.3 戴好头盔，开始采集 + 看实时数据

```bash
# 1. 让服务连接 OpenBCI 板子
curl -X POST http://127.0.0.1:8765/connect

# 2. 开始流
curl -X POST http://127.0.0.1:8765/start

# 3. 等 5 秒攒数据，然后看一眼
sleep 5
curl http://127.0.0.1:8765/snapshot   # 16 通道波形
curl http://127.0.0.1:8765/decision   # SSVEP 分类结果
```

如果 `/snapshot` 返回的 `channels` 里有非零的浮点数 → 真的在采集到信号了。

### 4.4 录一段 10 秒的数据（写到 SD 卡）

```bash
curl -X POST http://127.0.0.1:8765/record/start
sleep 10
curl -X POST http://127.0.0.1:8765/record/stop

# 看产物
ls -la ~/Neurostick/Neurostick-Pi-5/data/
```

应该能看到 `session_<时间戳>/` 里有 `samples.csv`、`decisions.ndjson`、`metadata.json` 三个文件。**这就是你要回传的产物。**

### 4.5 把测试结果填进文档回传

打开 `~/Neurostick/Neurostick-Pi-5/docs/HARDWARE_RESULTS.md`，把模板里的空白填进去：

- Pi 型号 + 内存
- OS 版本：`cat /etc/os-release | head -3`
- Docker 版本：`docker --version`
- `OPENBCI_DEVICE` 路径
- 容器里 BrainFlow 的架构：
  ```bash
  docker exec neurostick-pi5-edge file /opt/brainflow/lib/libBoardController.so
  ```
  应该是 `ARM aarch64`
- 30 分钟稳定性：让它跑 30 分钟然后 `docker stats neurostick-pi5-edge`，看内存有没有飞涨

填好之后 git push 回去，或者把文件发回来都行。

---

## 5. 出问题了怎么办

### 5.1 SSH 不上

- Pi 真的开机了吗？看红灯（电源）+ 绿灯（SD 活动）
- 网线插好了吗？路由器后台能看到这个 hostname 吗？
- WiFi 密码在 Imager 里写错了？拔 SD 卡重新烧

### 5.2 `pi5-bootstrap.sh` 卡在某一步

先在另一个 SSH 窗口看进度：

```bash
docker ps -a       # 容器有没有起来
docker logs $(docker ps -q | head -1)   # 看容器日志
```

如果是网络问题（拉 docker.io 镜像慢），可以挂代理或者改国内镜像源（`/etc/docker/daemon.json` 加 `registry-mirrors`），重启 docker 后再来一次。

### 5.3 `dongle 看不到`

```bash
groups   # 看自己有没有在 dialout 组里
sudo usermod -aG dialout $USER
# 然后退出 SSH 重新登录
```

### 5.4 容器里 BrainFlow 报错 "x86_64"

绝对不应该。如果真出现，截图发回来 — 这是 release 包的问题，不是你这边。

### 5.5 详细排错

`~/Neurostick/Neurostick-Pi-5/docs/TROUBLESHOOTING.md` 有更详细的每一种症状的修复。

---

## 6. 完了之后顺便玩一下

`/events` 是给 VR / 机械臂之类客户端用的实时推送。直接在浏览器开 Pi 的 IP 看：

```bash
# 笔记本上的浏览器开 http://<pi-ip>:8765/events
# 应该看到一行行 JSON 实时滚出来
```

更详细的协议看 `~/Neurostick/Neurostick-Pi-5/docs/VR_INTEGRATION.md`。

---

完成了告诉 Mikii，整个流程加上等编译 1.5-2.5 小时。
