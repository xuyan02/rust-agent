# IntrospectionBrain Implementation Summary

## 概述

IntrospectionBrain 是一个后台运行的第三大脑，负责观察、提炼和维护Bot的知识库与记忆。

### 三个 Brain 的对比

| Brain | Agent 类型 | 作用 | 输出方式 |
|-------|-----------|------|---------|
| **Conversation Brain** | LlmAgent | 与用户对话，协调任务 | `@recipient:` 消息协议 |
| **Work Brain** | ReActAgent | 执行复杂任务 | `[answer]` 结果 → WorkBrainSink → Conversation Brain |
| **Introspection Brain** | ReActAgent | 自省、知识提炼、记忆压缩 | `[answer]` 摘要 → IntrospectionBrainSink → Conversation Brain |

**为什么 Conversation Brain 使用 LlmAgent**：
- 需要快速响应用户，不需要复杂的 Think-Act 循环
- 主要负责协调和转发，而非深度推理
- 使用 `@recipient:` 协议输出，简单直接

## 架构设计

### 三个核心职责

1. **观察** - 读取 Conversation Brain 和 Work Brain 的历史记录
2. **提炼** - 将重要知识/经验提取到 Knowledge Base（深度记忆）
3. **压缩** - 当 Memory 超过 8000 tokens 时，压缩到 ~4000 tokens

### 触发机制

**1. 程序化触发**（代码调用）
- **定时触发**: 每30分钟调用 `bot.trigger_introspection()`
- **阈值触发**: Memory token > 8000 时调用 `bot.check_and_trigger_introspection()`

**2. Conversation Brain 主动触发** ⭐ NEW
- Conversation Brain 通过消息协议触发：`@introspection-brain: Perform introspection...`
- 场景：用户要求整理知识、完成重大里程碑、Memory 接近上限等

## 实现的组件

### 1. Knowledge Base (知识库)

**文件**: `crates/agent-bot/src/knowledge_base.rs`

- 使用 Markdown 文件 + 目录结构存储
- 位置: `.agent/{bot_name}/knowledge/`
- 完全由 IntrospectionBrain 动态管理结构

**方法**:
- `list_files()` - 列出所有 markdown 文件
- `list_dirs()` - 列出所有目录
- `read_file()` - 读取知识文件
- `write_file()` - 写入/更新知识文件
- `move_file()` - 重新组织结构
- `delete_file()` - 删除过时知识

### 2. Knowledge Tools (知识管理工具)

**文件**: `crates/agent-bot/src/knowledge_tools.rs`

提供给 IntrospectionBrain 的工具：
- `list-knowledge` - 列出所有知识文件和目录
- `read-knowledge` - 读取知识文件
- `write-knowledge` - 创建/更新知识文件（自动创建父目录）
- `move-knowledge` - 移动/重命名文件
- `delete-knowledge` - 删除文件

### 3. Memory Compression Tools (记忆压缩工具)

**文件**: `crates/agent-bot/src/memory_tool.rs` (扩展)

新增功能：
- `get-memory-size` - 获取当前 memory token 数（粗略估算）
- `replace-memories` - 替换所有 memory（用于压缩）

**MemoryState 新增方法**:
- `replace_all(Vec<String>)` - 替换所有记忆
- `count_tokens()` - 统计 token 数（1 token ≈ 4 chars）

### 4. History Reading Tools (历史读取工具)

**文件**: `crates/agent-bot/src/history_tool.rs`

只读访问其他 brains 的历史，**按需读取被压缩归档的历史**：

**最近历史**（仅 history.yaml）：
- `read-conv-history` - 读取 Conversation Brain 最近历史
- `read-work-history` - 读取 Work Brain 最近历史

**归档访问**（按需读取特定归档）：
- `read-conv-archive(filename)` - 读取指定的 conversation 归档文件
- `read-work-archive(filename)` - 读取指定的 work 归档文件

**归档列表**：
- `list-conv-archives` - 列出所有 conversation 归档文件
- `list-work-archives` - 列出所有 work 归档文件

返回格式化的历史摘要，长内容自动截断。

**工作流程**：
1. 读取最近历史（`read-*-history`）
2. 查找压缩引用：`[Previous N messages archived to history/{filename}]`
3. 按需读取归档（`read-*-archive` 使用发现的 filename）
4. 提取知识

**不读取完整历史**：避免一次性加载所有归档（太长），而是根据引用按需访问。

### 5. Introspection Brain Prompt (提示词)

**文件**: `crates/agent-bot/prompts/introspection_brain.md`

定义了 IntrospectionBrain 的角色和工作模式：
- Observer: 监控历史，识别模式和教训
- Curator: 提取知识并组织到 Knowledge Base
- Compressor: 使用混合策略压缩 Memory

**混合压缩策略**:
- 时间: 归档旧的记忆到 Knowledge Base
- 重要性: 保留关键上下文，删除琐碎细节
- 合并: 将相关记忆合并为简洁摘要

### 6. Bot Integration (集成到 Bot)

**修改文件**: `crates/agent-bot/src/bot.rs`

**新增结构**:
```rust
struct Inner {
    ...
    introspection_brain: Rc<RefCell<Option<Box<Brain>>>>,
    knowledge_base: Rc<KnowledgeBase>,
}

pub struct Bot {
    ...
    knowledge_base: Rc<KnowledgeBase>,
}
```

**新增方法**:
- `trigger_introspection()` - 手动触发 introspection
- `should_trigger_introspection()` - 检查是否需要触发（memory > 8000 tokens）
- `check_and_trigger_introspection()` - 检查并自动触发

**Session 创建**:
- 使用 `ReActAgent`（Think-Act 循环，适合复杂推理和多步工具调用）
- 超时时间: 10 分钟
- 使用 `IntrospectionBrainSink`（将结果发送回 Conversation Brain）
- 独立的历史存储: `.agent/{bot}/introspection/history.yaml`

**为什么使用 ReActAgent**:
- 需要多步推理：思考如何组织知识、评估记忆重要性、决定压缩策略
- 复杂的工具调用序列：读取历史 → 分析内容 → 提取知识 → 写入文件 → 压缩记忆
- Think-Act 循环适合这种需要反复观察和决策的任务

**输出格式**:
- IntrospectionBrain 输出 `[answer]` 格式的简短摘要（2-4 句话）
- Brain 自动去掉 `[answer]` 前缀
- 摘要通过 `IntrospectionBrainSink` 转发到 Conversation Brain
- Conversation Brain 使用 `@user:` 将结果报告给用户
- 类似 Work Brain 的工作方式，不使用 `@recipient` 协议

## 使用示例

### 手动触发

```rust
let bot = Bot::new(runtime, "MyBot", "gpt-4o", tools, sink)?;

// 手动触发一次 introspection
bot.trigger_introspection();

// 检查并在需要时触发
bot.check_and_trigger_introspection();

// 检查是否应该触发
if bot.should_trigger_introspection() {
    println!("Memory is getting full, introspection recommended");
}
```

### Conversation Brain 主动触发 ⭐ NEW

Conversation Brain 可以在对话中主动触发 IntrospectionBrain：

```
用户: 请整理一下我们讨论的 Rust 知识

Conversation Brain 输出:
@introspection-brain: Perform introspection and knowledge extraction, focusing on Rust concepts we discussed.
@user: 好的，我正在整理知识库并压缩记忆。
```

**完整工作流程**：
1. Conversation Brain 向 `@introspection-brain` 发送消息
2. 消息通过 BrainToBotSink 路由到 IntrospectionBrain
3. IntrospectionBrain 在后台执行：
   - 读取历史（包括归档）
   - 提取知识到 Knowledge Base
   - 压缩 Memory（如果需要）
4. IntrospectionBrain 输出摘要（2-4句话）
5. 摘要通过 IntrospectionBrainSink 发送回 Conversation Brain
6. Conversation Brain 收到 "Introspection brain result: ..."
7. Conversation Brain 将结果转发给用户：`@user: [摘要]`

**示例输出**：
```
IntrospectionBrain 输出:
Extracted 3 new knowledge entries about Rust async patterns to tech/rust/.
Compressed memory from 9500 to 4200 tokens.
Identified recurring authentication error pattern worth documenting.

↓ 通过 IntrospectionBrainSink ↓

Conversation Brain 收到:
Introspection brain result:
Extracted 3 new knowledge entries about Rust async patterns to tech/rust/.
Compressed memory from 9500 to 4200 tokens.
Identified recurring authentication error pattern worth documenting.

↓ 解析并转发 ↓

Conversation Brain 输出:
@user: 知识整理完成。提取了3条关于Rust async的知识，记忆已压缩至4200 tokens。
```

**适用场景**：
- 用户明确要求整理知识
- Memory 接近上限（Conversation Brain 可以主动检查）
- 完成重大里程碑（如完成大型项目）
- 发现值得记录的重复模式

### 自动触发（需要集成到事件循环）

建议在以下时机调用 `check_and_trigger_introspection()`:
- 每次对话完成后
- 每次 work brain 任务完成后
- 每 30 分钟（定时器）

## 知识库结构示例

```
.agent/MyBot/knowledge/
├── tech/
│   ├── rust/
│   │   ├── async_traits.md
│   │   └── ownership.md
│   └── python/
│       └── decorators.md
├── workflows/
│   ├── deployment.md
│   └── testing.md
├── lessons/
│   ├── 2024_02_bug_fixes.md
│   └── performance_optimization.md
└── domain/
    ├── business_logic.md
    └── api_design.md
```

## Memory 压缩示例

**压缩前 (10 条记忆, ~10000 tokens)**:
1. "Rust uses borrowing to ensure memory safety"
2. "async/await requires the tokio runtime"
3. "We use axum for web framework"
4. "Database uses PostgreSQL with sqlx"
5. "Tests run with cargo test"
6. "CI/CD uses GitHub Actions"
7. "Deployment target is AWS ECS"
8. "Logging uses tracing crate"
9. "Config loaded from config.toml"
10. "API rate limit is 1000 req/min"

**压缩后 (4 条记忆, ~4000 tokens)**:
1. "Tech stack: Rust + Tokio + Axum + PostgreSQL/sqlx"
2. "Infrastructure: GitHub Actions CI/CD → AWS ECS deployment"
3. "Development: cargo test for testing, tracing for logs, config.toml for config"
4. "API constraints: 1000 req/min rate limit"

**归档到 Knowledge Base**:
- `tech/rust/async_runtime.md` - async/await 和 tokio 详细信息
- `tech/web_stack.md` - Axum 使用方法
- `infrastructure/deployment.md` - AWS ECS 部署流程

## 历史归档结构

当 history 被压缩时，旧消息会存储到归档中：

```
.agent/MyBot/
├── conv/
│   ├── history.yaml          # 最近的消息（未压缩）
│   └── history/              # 归档目录
│       ├── 1709000000.yaml   # 归档1（时间戳命名）
│       └── 1709010000.yaml   # 归档2
└── work/
    ├── history.yaml
    └── history/
        └── 1709005000.yaml
```

**归档文件格式**：
```yaml
compressed_at: "2024-02-27T12:00:00+08:00"
message_count: 50
estimated_tokens: 4000
messages:
  - role: User
    content: ...
  - role: Assistant
    content: ...
```

IntrospectionBrain 可以按需读取归档，避免一次性加载过长的历史。

## 使用归档的工作流程

### 场景：提取历史知识

**步骤 1**: 读取最近历史
```
read-conv-history
```

**返回**:
```
## Recent History (history.yaml)

1. System: ...
2. User: 帮我分析一下Rust的async特性
3. Assistant: [Previous 50 messages archived to history/1709000000.yaml]

Summary:
讨论了Rust的ownership机制、借用检查器和生命周期系统...

Conversation continues...
4. Assistant: 好的，我来分析async特性...
```

**步骤 2**: 发现归档引用
看到 `[Previous 50 messages archived to history/1709000000.yaml]`

**步骤 3**: 读取该归档
```
read-conv-archive("1709000000.yaml")
```

**返回**:
```
## Archive: 1709000000.yaml
Compressed at: 2024-02-27T10:00:00+08:00
Messages: 50, Tokens: ~4000

1. User: Rust的ownership是什么？
2. Assistant: Ownership是Rust的核心概念...
[归档中的详细内容]
```

**步骤 4**: 提取知识
从归档中提取 ownership、borrowing 等概念，写入 Knowledge Base:
```
write-knowledge("tech/rust/ownership.md", "...")
```

这样可以避免一次性读取所有历史（可能有几十个归档，太长），而是根据需要按需访问。

## 待完成

1. **定时触发器**: 需要在 Runtime 或 event loop 中添加 30 分钟定时器
2. **自动触发集成**: 在适当的时机调用 `check_and_trigger_introspection()`
3. **监控和日志**: 添加 metrics 来跟踪 knowledge base 大小和 memory 压缩效果

## 测试

所有现有测试通过：
```bash
cargo test --package agent-bot
```

建议添加的测试：
- Knowledge Base CRUD 操作
- Memory 压缩逻辑
- History 读取工具
- IntrospectionBrain 端到端测试

## 文件清单

**新增文件**:
- `crates/agent-bot/src/knowledge_base.rs`
- `crates/agent-bot/src/knowledge_tools.rs`
- `crates/agent-bot/src/history_tool.rs`
- `crates/agent-bot/prompts/introspection_brain.md`

**修改文件**:
- `crates/agent-bot/src/bot.rs` - 集成 IntrospectionBrain，添加 IntrospectionBrainSink，消息路由
- `crates/agent-bot/src/brain.rs` - 提取输出时去掉 ReAct 前缀（`[answer]`、`[think]`、`[act]`）
- `crates/agent-bot/src/memory_tool.rs` - 添加压缩工具
- `crates/agent-bot/src/lib.rs` - 导出新模块
- `crates/agent-bot/prompts/conversation_brain.md` - 添加触发 introspection 的说明和结果处理
- `crates/agent-bot/prompts/introspection_brain.md` - 定义输出格式（简短摘要）
- `crates/agent-core/src/history/compression.rs` - 修复字符串截断 bug
- `crates/agent-core/src/tools/deep_think.rs` - 修复字符串截断 bug

## 总结

IntrospectionBrain 实现完整，提供了：
- ✅ 知识库管理（Markdown + 目录结构）
- ✅ 历史观察能力（按需读取归档）
- ✅ Memory 压缩能力
- ✅ 多种触发机制：
  - 程序化触发：`bot.trigger_introspection()` / `bot.check_and_trigger_introspection()`
  - **Conversation Brain 主动触发**：`@introspection-brain: ...` ⭐
  - 阈值触发：Memory > 8000 tokens
- ✅ 完整的工具集
- ✅ 后台运行（不干扰用户交互）
- ✅ **结果反馈**：通过 IntrospectionBrainSink 将摘要发回 Conversation Brain ⭐

**工作方式类似 Work Brain**：
- Conversation Brain 委派任务
- IntrospectionBrain 后台执行
- 完成后输出简短摘要
- 摘要发送回 Conversation Brain
- Conversation Brain 转达给用户
