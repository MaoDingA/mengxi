# Changelog

All notable changes to this project will be documented in this file.

## [1.0.0.0] - 2026-03-22

### Added

- **CLI 框架**: 9 个子命令的完整 CLI 工具（import, search, export, info, tag, lut-diff, lut-dep, stats, config），支持交互模式和脚本模式，JSON/文本双格式输出
- **项目导入**: 支持 DPX（8/10/12/16-bit）、EXR（half-float、多压缩格式）、MOV 文件格式自动检测与导入，支持断点续传和进度显示
- **色彩指纹提取**: 通过 MoonBit FFI 桥接，从导入文件中提取直方图、色彩空间分布、关键帧色彩特征等指纹数据
- **ACES 1.3 色彩科学**: 完整的 ACES 色彩空间转换引擎，支持 ACEScg、ACES2065-1、Rec.709 之间的数学精确转换
- **LUT 管理**: 多格式 LUT 文件读写（.cube/.3dl/.look/.csp/ASC-CDL）、PowerGrade 只读解析、LUT 导出、LUT 对比差异报告、LUT 依赖追踪
- **直方图搜索**: 基于色彩直方图的相似度搜索，支持项目范围过滤和结果数量配置
- **图像嵌入搜索**: Python AI 子进程 + ONNX 推理，基于图像参考的语义视觉相似度搜索，嵌入向量缓存
- **标签搜索**: 基于语义标签的搜索、搜索结果接受/拒绝反馈
- **AI 标签生成**: 自动为索引项目生成语义标签，支持事后批量生成
- **手动标签管理**: 标签的增删改查、来源追踪（AI/手动）、标签重命名
- **标签校准学习**: AI 从用户标签修正中学习，改进未来标签生成的个性化词汇
- **会话跟踪**: CLI 使用会话记录，命令序列和时间测量
- **使用统计**: 搜索命中率、校准活动指标、词汇增长率和复用率趋势、用户维度统计与报表
- **嵌入式数据库**: SQLite (rusqlite) WAL 模式，16 个迁移文件，8 张数据表，13 个索引
- **Python AI 子进程**: JSON-RPC 通信，空闲超时自动回收，崩溃自动重启，模型可插拔
- **TOML 配置**: 单文件配置 `~/.mengxi/config`，合理的默认值

### Removed

_(none)_

### Changed

_(none)_

### Fixed

- 修复 Python 可执行文件查找逻辑，优先查找本地虚拟环境
- 修复嵌入向量反序列化和搜索逻辑错误
- 修复会话 ID 生成及搜索时间计算逻辑
- 修复标签操作的校准记录逻辑
- 修复多个 code review 发现的 patch issues（LUT diff、search、tag 模块）
- 改进 AI 标签生成流程的错误处理
- 改进导入命令的错误处理和进度显示
- 优化导出功能并增强错误处理
