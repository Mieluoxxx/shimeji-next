# Shimeji Next 路线图

> 从 Shimeji-ee（Java）出发的原生 Rust 重写。v0.3 起不再解析 XML/JS 配置：
> 皮肤 = 46 张约定帧 + 可选 skin.toml，行为逻辑用类型化 Rust 实现，时间单位是秒、物理按 dt 积分。

## v0.3 原生引擎（当前）

### 模拟层 `sim/`
- [x] 60Hz 固定步长，物理帧率无关（测试：25/60/240Hz 下落地时间一致）
- [x] 原版 40ms tick 参数全部换算：时长 ×0.04s，速度 ×25 px/s，阻力/重力改为连续指数衰减（终端速度保持 500 px/s）
- [x] 全部 56 个默认行为与 30 个动画剪辑；权重、隐藏、后续行为链（坐下→晃腿/躺下）与原版一致
- [x] 贴边语义（地面/墙/天花板/窗口四边，窗口移动时一起带走），失足→下落
- [x] 拖拽：每帧跟手、弹簧脚选姿势、静止 10s 挣扎、挣扎完挣脱；松手按光标速度（px/s）甩出
- [x] 扛窗口/扔窗口（本地预测窗口位置 + 30Hz 节流 AX 调用）
- [x] 分裂、拉出同伴（50 只上限）
- [x] 修正原版笔误：`Math.random*100` 得到 NaN 导致的错误目标点

### 资源 `assets/`
- [x] 46 帧约定 + skin.toml（名称/作者/授权/缩放/速度/锚点/禁用行为/权重）
- [x] 内置皮肤 + 用户皮肤目录合并，同名用户优先；托盘打开文件夹、刷新列表
- [x] settings.toml（serde），i18n/*.toml（21 种语言，新增中文行为名）

### 平台与界面
- [x] macOS：visibleFrame 工作区（站在 Dock 上）、CGEvent 光标、CGWindowList 前台窗口、AX 移动窗口
- [x] 透明置顶窗口、逐像素点击穿透、wgpu alpha 模式协商
- [x] 托盘菜单与右键菜单（触发行为、允许的行为、暂停、去除…）

### 工程
- [x] lib + bin 拆分，模拟层可在 `tests/` 里无窗口测试（12 项，含 4 只×20 分钟行为覆盖）
- [x] 依赖瘦身：去掉 QuickJS、roxmltree、rodio；.app 22MB / dmg 17MB
- [x] `build_app.sh`：universal 二进制、dmg、可选 Developer ID 签名与公证

## 待办

- [ ] Windows 原生平台层（GetCursorPos / 工作区 / EnumWindows / SetWindowPos）与透明合成实测
- [ ] 跟随屏幕刷新率（120Hz ProMotion）而不是固定 60Hz
- [ ] 设置窗口（缩放、透明度目前需改 settings.toml）
- [ ] skin.toml 支持自定义动画（使用 47 号以后的帧）
- [ ] 音效（skin.toml 声明，按帧触发）
- [ ] 多显示器实测、空闲时降低 CPU（宠物静止时不必 60fps 重排窗口）

## 与 Shimeji-ee 的差异

- 不读取 actions.xml / behaviors.xml；自带特殊动作配置的社区皮肤会按标准行为运行。
- 原版 schema 写的是 NextBehaviorList、自带配置却用 NextBehavior，Java 版因此丢掉了全部后续行为链；这里按配置本意实现。
- 贴边判定用精确相等（Java fork 的 ±2 像素容差会导致悬空）。
- 扛窗口时允许 24 像素错位：窗口位置异步回报，逐像素比对在 60fps 下会误判掉落。
