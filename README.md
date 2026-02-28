# agent-bot（Rust）

一个用于构建与试验 **多智能体（Leader/Workers）协作** 的 Rust 工作区。

- **Leader（领导者）**：唯一可以直接与用户对话的机器人。
- **Workers（执行者）**：按需创建，用于拆分任务、执行子任务并向 Leader 汇报。
- **team-cli**：本项目自带的交互式 Playground，用来在本地快速体验协作模型。

许可证：**MIT OR Apache-2.0**。

## Crates 说明

- `agent-core` — 编排与运行时基础设施（runtime、session、tool 调度、LLM provider 抽象等）
- `agent-bot` — 构建在 `agent-core` 之上的多机器人运行时（Bot/Brain/Team）
- `team-cli` — 交互式 CLI，用于测试 Team 协作
- `agent-cli`、`brain-cli` — 其他 CLI（实验性 / WIP）
- `agent-macros` — 工作区内使用的过程宏
- `agent-tests` — 工作区测试

## 特性

- **Team 协作（Leader / Workers）**
  - 单一 Leader 负责与用户交互、拆分任务与路由消息。
  - Workers 执行子任务并回传结果。
  - Team 统一管理 bot 生命周期与消息流转。

- **可组合的工具系统（Tool System）**
  - 通过函数名分发工具调用。
  - 工具既可作为 session 默认工具，也可在单次运行上下文中覆写/扩展。

- **CLI Playground**
  - `team-cli` 提供 REPL 式交互。
  - 默认启用工具：文件系统工具 + Shell 工具（见 `crates/team-cli/src/main.rs`）。

## 快速开始（Quickstart）

### 1）前置条件

- Rust 工具链（工作区使用 **edition 2024**）
- 一个 OpenAI 兼容的 API Endpoint 与 API Key

### 2）创建配置文件

在仓库根目录创建 `.agent/agent.yaml`：

```yaml
model: gpt-4o

openai:
  base_url: https://api.openai.com
  api_key: sk-your-api-key-here
  # 可选：
  # model_provider_id: default
```

说明：

- `model`：`team-cli` **必填**（否则启动会报缺失 model）。
- `openai.model_provider_id`：可选字段。

### 3）运行 Team CLI

```bash
cargo run --package team-cli
```

默认值：

- 用户名：`Alice`
- Leader 名：`LeaderBot`
- 配置路径：`.agent/agent.yaml`

交互中可用命令：

- 直接输入任意文本：发送给 Leader
- `status`：输出当前团队状态
- `exit`：退出并关闭 Team

## CLI 用法

```text
cargo run --package team-cli -- [--user <name>] [--leader <name>] [--cfg <path>] [--timeout-ms <n>]

--user <name>       设置用户名（默认：Alice）
--leader <name>     设置 Leader 名称（默认：LeaderBot）
--cfg <path>        配置文件路径（默认：.agent/agent.yaml）
--timeout-ms <n>    单次用户消息等待超时（可选）
-h, --help          显示帮助
```

示例：

```bash
cargo run --package team-cli -- --user Bob --leader CoordinatorBot --timeout-ms 30000
```

## Team 的机器人通信协议

`team-cli` 与 Team runtime 使用一个简单的消息信封协议：

```json
{"to":"recipient_name","content":"message content"}
```

约定：

- 只有 **Leader** 应该直接对用户发言。
- Worker 主要与 Leader（或其他 Worker）通信，由 Team 做路由。

## 开发

### 构建

```bash
cargo build
```

### 日志

CLI 使用 `tracing`。通过 `RUST_LOG` 控制日志级别：

```bash
RUST_LOG=info  cargo run --package team-cli
RUST_LOG=debug cargo run --package team-cli
```

### 格式化与静态检查

```bash
cargo fmt
cargo clippy
```

## 测试

```bash
cargo test
```

## 文档

更多设计与实现文档请参阅 [`docs/`](docs/README.md)，包含：

- **架构设计** — Brain 架构、Introspection Brain、Memory Tool 等
- **持久化** — 历史记录持久化、压缩、Goal/Memory 持久化等
- **Team 功能** — 多 Bot 协作实现、CLI、工具管理等
- **重构记录** — Brain 重构、统一配置、改进计划等

## License

本项目采用双许可证：**MIT OR Apache-2.0**（见工作区 `Cargo.toml`）。
