# Dead Code Analysis — team-cli 入口视角

> 基于 `cargo build --bin team-cli` 干净编译后的警告 + 手动依赖链追踪

## 1. 编译器直接报告的 Dead Code（8 条 warning）

### agent-core（3 条）

| 位置 | 类型 | 详情 |
|------|------|------|
| `tools/deep_think.rs:2` | unused import | `AgentContextBuilder`, `InMemoryHistory`（代码用了 `crate::` 路径） |
| `agent.rs:10` | dead constant | `MAX_TOOL_ITERATIONS = 50`（全项目零引用，ReActAgent 无上限） |
| `history/archiver.rs:86,105` | dead methods | `load()`, `list_archives()`（只用了 `new/generate_filename/save`） |

### agent-bot（5 条）

| 位置 | 类型 | 详情 |
|------|------|------|
| `bot.rs:46` | dead field | `ConversationBrainSink.bot_name`（赋值但从未读取） |
| `bot.rs:52` | dead field | `WorkBrainSink.bot_name`（赋值但从未读取） |
| `bot.rs:200` | dead fields | `Inner.goal_state`, `Inner.memory_state`, `Inner.knowledge_base` |
| `bot.rs:207` | dead fields | `Bot.goal_state`, `Bot.knowledge_base` |
| `brain.rs:159` | dead field | `Inner.name`（赋值但从未读取） |

---

## 2. 手动追踪发现的 Dead Code（编译器不报 warning 的）

### 🔴 高优先级：整个 support/ 模块（191 行）

`crates/agent-core/src/support/` 从 team-cli 运行时完全不可达：

| 子模块 | 行数 | 说明 |
|--------|------|------|
| `console/mod.rs` | 32 | `Console` trait + `CliConsole`，零运行时引用 |
| `skill/mod.rs` | 69 | `SkillRegistry`，零运行时引用 |
| `storage/mod.rs` | 45 | `JsonFileStorage`，零运行时引用 |
| `runtime/mod.rs` | 41 | `Runtime<C>` + 重复的 `Console` trait，仅 1 个测试引用 |
| `mod.rs` | 4 | 模块声明 |

> 唯一的外部引用是 `test_cli_plan_command.rs` 使用了 `runtime::Console` 和 `Runtime`。

### 🟡 中优先级：残留空文件

| 文件 | 说明 |
|------|------|
| `agent-bot/src/brain_driver.rs` | 0 字节空文件，未被 `lib.rs` 声明为 mod |

### 🟡 中优先级：仅测试使用的 pub 导出

这些在运行时是 dead code，但被测试引用，删除需同步修改测试：

| 导出 | 来源 | 引用方 |
|------|------|--------|
| `MacroExampleTool` | `tools/macro_example.rs` (22 行) | `test_tool_macro.rs` |
| `AgentContextParent` | `agent_context.rs` | 仅 crate 内部 |
| `SlidingWindowConfig` | `history/mod.rs` | 零外部引用 |
| `find_tool_for_function` | `tool_dispatch.rs` | `test_agent_context_tool_precedence.rs` |

### 🟢 低优先级：未使用的 pub 方法

编译器对 `pub` 方法不报 warning，但这些在整个项目中零调用：

| 方法 | 文件 |
|------|------|
| `DataNode::remove()` | `data_store.rs:206` |
| `DataStore::children()` | `data_store.rs:312` |
| `DataStore::subdirs()` | `data_store.rs:346` |
| `AgentContextBuilder::add_system_segment()` | `agent_context.rs:144` |
| `LlmContext::clear()` | `llm/context.rs:56`（仅 1 测试） |

---

## 3. 有意抑制的 Dead Code（#[allow(dead_code)]）

| 位置 | 字段 | 原因 |
|------|------|------|
| `team-cli/src/main.rs:43` | `AgentCfg.model` | 从 YAML 反序列化但通过 `.clone()` 单独读取 |
| `brain-cli/src/main.rs:22` | `AgentCfg.model` | 同上 |
| `agent-bot/examples/brain_driver_cli.rs:20` | `AgentCfg.model` | 同上 |

---

## 4. 汇总

| 类别 | 数量 | 估计行数 |
|------|------|----------|
| 编译器报告的 dead code | 8 条 warning | ~50 行 |
| support/ 整模块 dead | 5 个文件 | 191 行 |
| 残留空文件 | 1 个文件 | 0 行 |
| 仅测试用的 dead export | 4 项 | ~30 行 |
| 未使用的 pub 方法 | 5 个方法 | ~60 行 |
| **合计** | | **~330 行** |

## 5. 建议清理操作

1. **立即可做**：删除 `support/` 目录（移除 `pub mod support;`），删除 `brain_driver.rs`
2. **快速修复**：`cargo fix --lib -p agent-core -p agent-bot` 清理 unused imports
3. **需评估**：`MAX_TOOL_ITERATIONS` 应被 ReActAgent 使用（安全上限），或确认删除
4. **需评估**：`archiver.load()` / `list_archives()` 可能是未来功能，标注 `#[allow(dead_code)]` 或删除
5. **需评估**：bot.rs 中的 dead fields 可能是架构预留，需确认是否有后续计划
