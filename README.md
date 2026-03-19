# Mengxi (梦溪)

面向专业影视后期制作（数字中间片 / DI）流程的命令行调色管线管理平台。Mengxi 帮助调色师通过图像相似度匹配检索历史项目库，并将匹配的调色风格导出为 LUT 文件，直接导入 DaVinci Resolve 使用。

## 解决的问题

当导演描述一种期望的画面氛围时，调色师通常需要手动将描述转化为技术参数——每次沟通约 30 分钟的创作瓶颈。现有工具（包括 DaVinci Resolve 自带的项目库、Gallery 和 PowerGrade）均不支持跨项目的调色风格搜索或语义检索。

**Mengxi 将定调时间从约 30 分钟缩短至 1 分钟以内。**

## 核心功能

- **项目导入** — 导入 DPX/EXR/MOV 项目文件夹，自动识别格式、提取关键帧和色彩指纹
- **色彩指纹提取** — 提取丰富的色彩元数据（直方图、色彩空间分布、关键帧特征），存入本地嵌入式数据库
- **基于图像的相似度搜索** — 上传参考图片，通过直方图匹配和 AI 向量嵌入，返回 Top-N 匹配结果
- **LUT 导出** — 将匹配的风格导出为 `.cube`、`.3dl`、`.look`、`.csp` 和 ASC-CDL 格式的 LUT 文件，可直接导入 DaVinci Resolve
- **LUT 版本管理** — LUT 文件差异对比和依赖关系追踪
- **人机协同标签校准** — AI 自动生成语义标签，调色师修正后系统持续学习优化
- **命令行界面** — 9 个子命令（`import`、`search`、`export`、`info`、`tag`、`lut-diff`、`lut-dep`、`stats`、`config`），支持交互模式和脚本批处理模式

## 架构

Mengxi 采用三层语言架构，各取所长：

```
┌─────────────────────────────────────────────────┐
│  Rust — CLI 外壳、系统 I/O、FFI 桥接             │
│  clap · rusqlite · dpx/openexr crates            │
├─────────────────────────────────────────────────┤
│  MoonBit — 核心算法                               │
│  ACES 1.3 · 色彩科学 · LUT 引擎                  │
│  类型安全的色彩空间（编译期保证）                    │
├─────────────────────────────────────────────────┤
│  Python — AI 推理（可选子进程）                    │
│  ONNX Runtime · 向量嵌入 · 标签生成               │
└─────────────────────────────────────────────────┘
```

- **Rust** 负责 CLI、文件格式解码（DPX/EXR/MOV）、数据库操作和 Python AI 子进程管理
- **MoonBit** 实现纯色彩科学函数——无 I/O、无状态，所有接口均为数值数组输入输出，确保可测试性
- **Python** 作为长驻子进程运行 AI 增强功能（向量嵌入生成、标签预测）。核心流程无需 Python 即可运行——Rust + MoonBit 独立完成导入、指纹提取、直方图搜索和 LUT 导出

### 关键设计决策

- **FFI 边界**：图像像素数据不跨越 FFI——仅传递预计算的数值数组
- **类型安全的色彩空间**：MoonBit 类型系统在编译期强制区分 Linear/Log/Video，从根源上杜绝一整类色彩科学 bug
- **嵌入式 SQLite**：单文件数据库，WAL 模式，零外部依赖
- **Python 可选**：AI 功能可优雅降级；无 Python 环境时工具仍完全可用

## 项目结构

```
mengxi/
├── Cargo.toml              # Rust workspace 根配置
├── build.rs                # 通过 FFI 链接 libmoonbit_core.a
├── migrations/             # SQL 迁移文件
├── crates/
│   ├── cli/                # CLI 入口（9 个子命令）
│   ├── core/               # 领域逻辑、数据库、Python 桥接、分析统计
│   └── format/             # 格式解码器（DPX, EXR, MOV, LUT, PowerGrade）
├── moonbit/                # MoonBit 核心算法
│   └── src/                # color_science, fingerprint, similarity, lut_engine, types
├── python/                 # AI 推理子进程
│   └── mengxi_ai/          # main.py, embedding.py, tagging.py, models.py
└── tests/                  # 集成测试 + 测试数据
```

## 开发状态

**规划完成，开发进行中。**

- [x] 产品需求文档
- [x] 架构设计
- [x] Epics & Stories（5 个 Epic，21 个 Story）
- [ ] Sprint 1：CLI 基础 & 项目导入
- [ ] Sprint 2：指纹引擎 & 搜索
- [ ] Sprint 3：LUT 引擎 & 导出
- [ ] Sprint 4：AI 标签增强 & 校准
- [ ] Sprint 5：分析统计 & 报告

## 快速开始

> _前置要求：[Rust](https://rustup.rs/) nightly、[MoonBit](https://moonbitlang.com/) 工具链（v0.8.x）、Python 3.11+（可选，用于 AI 功能）_

```bash
# 克隆仓库
git clone https://github.com/MaoDingA/mengxi.git
cd mengxi

# 构建
cargo build --release

# 运行
cargo run -- import /path/to/project
cargo run -- search /path/to/reference.png --top 5
cargo run -- export --match 1 --format cube --output style.cube
```

## 路线图

| 阶段 | 重点 |
|------|------|
| **MVP**（4 周） | 核心 7 项功能——导入、指纹、搜索、导出、LUT 对比、标签校准、CLI |
| **成长期**（第 2–6 月） | 自然语言搜索、增量索引、gRPC DaVinci 集成、TUI 仪表盘 |
| **扩展期**（第 6–12 月+） | GUI 界面、风格分析教学、DIT 现场集成、流媒体平台审片 |

## 参与贡献

欢迎贡献代码，请按以下步骤操作：

1. Fork 本仓库
2. 创建功能分支（`git checkout -b feature/your-feature`）
3. 进行开发并编写测试
4. 确保所有测试通过（`cargo test`）
5. 提交 Pull Request

## 许可证

本项目基于 [MIT 许可证](LICENSE) 开源。

## 作者

**毛丁 (Mao Ding)** — 调色师，拥有丰富的国内顶级影视项目调色经验，代表作品包括《流浪地球2》、《消失的她》、《与凤行》。
