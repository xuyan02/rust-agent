# History Compression

## 概述

PersistentHistory现在支持自动压缩功能，当对话历史超过token阈值时，自动将旧消息归档并生成摘要，减少上下文长度。

## 特性

### ✅ 自动触发
- 基于token数量估算（不是消息条数）
- 配置灵活的触发阈值

### ✅ 智能压缩
- 压缩旧消息，保留最近消息
- 生成摘要系统消息
- 原始消息归档到文件

### ✅ 文件归档
- 归档文件使用时间戳命名
- 包含完整的元数据（时间、消息数、token数）
- YAML格式，易于查看和恢复

## 配置

### 默认配置

```rust
HistoryCompressionConfig {
    compress_threshold_tokens: 20000,  // 超过20K tokens触发压缩
    compress_target_tokens: 16000,     // 每次压缩约16K tokens
    keep_recent_tokens: 4000,          // 保留最近4K tokens不压缩
    enabled: true,                     // 启用压缩
}
```

### 使用示例

```rust
use agent_core::{PersistentHistory, HistoryCompressionConfig};

// 1. 使用默认配置
let history = PersistentHistory::new();

// 2. 自定义配置
let config = HistoryCompressionConfig {
    compress_threshold_tokens: 15000,
    compress_target_tokens: 12000,
    keep_recent_tokens: 3000,
    enabled: true,
};
let history = PersistentHistory::new_with_config(100, config);

// 3. 禁用压缩
let history = PersistentHistory::new().without_compression();
```

## 工作流程

```
1. 对话进行中...
   messages: [msg1, msg2, ..., msg50]
   total_tokens: 22000  ← 超过阈值(20000)

2. 触发压缩
   - 计算压缩范围：前30条消息（~16K tokens）
   - 保留最近20条消息（~4K tokens）

3. 归档旧消息
   saved to: .agent/BotName/history/1709012345.yaml

   ArchivedHistory {
       compressed_at: "2026-02-26T10:30:45+08:00",
       message_count: 30,
       estimated_tokens: 16234,
       messages: [msg1, msg2, ..., msg30]
   }

4. 生成摘要消息
   system_msg: |
     === Compressed History ===
     Archive: history/1709012345.yaml
     Messages: 30
     Tokens: ~16234

     Summary: [压缩的对话摘要]

5. 新的历史结构
   messages: [summary_msg, msg31, ..., msg50]
   total_tokens: ~6000  ← 大幅减少
```

## 文件结构

```
.agent/
  LeaderBot/
    history.yaml              # 当前活跃历史（含摘要+未压缩消息）
    history/
      1709012345.yaml         # 第一次压缩的旧消息
      1709023456.yaml         # 第二次压缩的旧消息
      1709034567.yaml         # 第三次压缩的旧消息
```

### history.yaml 格式

```yaml
- role: System
  content: !Text |
    === Compressed History ===
    Archive: history/1709012345.yaml
    Messages: 30
    Tokens: ~16234

    Summary: The conversation covered...
- role: User
  content: !Text "Recent message..."
- role: Assistant
  content: !Text "Recent response..."
```

### history/1709012345.yaml 格式

```yaml
compressed_at: "2026-02-26T10:30:45+08:00"
message_count: 30
estimated_tokens: 16234
messages:
  - role: User
    content: !Text "Old message 1..."
  - role: Assistant
    content: !Text "Old response 1..."
  # ... 30 messages total
```

## Token估算

### 算法

使用启发式规则估算token数：
- **ASCII字符**: ~4字符 = 1 token
- **CJK字符**: ~1.5字符 = 1 token
- **混合文本**: 加权平均

### 示例

```rust
use agent_core::estimate_tokens;

let text1 = "Hello world";
let tokens1 = estimate_tokens(text1);  // ≈ 3 tokens

let text2 = "你好世界";
let tokens2 = estimate_tokens(text2);  // ≈ 3 tokens

let text3 = "Hello 世界";
let tokens3 = estimate_tokens(text3);  // ≈ 3 tokens
```

## Bot集成

Bot已自动启用压缩（见`crates/agent-bot/src/bot.rs`）：

```rust
let compression_config = agent_core::HistoryCompressionConfig {
    compress_threshold_tokens: 20000,
    compress_target_tokens: 16000,
    keep_recent_tokens: 4000,
    enabled: true,
};
main_brain_builder = main_brain_builder
    .set_history(Box::new(
        agent_core::PersistentHistory::new_with_config(100, compression_config)
    ));
```

## 测试

```bash
# 运行team-cli
cargo run -p team-cli

# 进行大量对话，触发压缩...

# 检查历史归档文件
ls -la .agent/LeaderBot/history/
cat .agent/LeaderBot/history/1709012345.yaml

# 检查当前历史（应该包含摘要+未压缩消息）
cat .agent/LeaderBot/history.yaml
```

## 性能考虑

### Token估算开销
- **极低**: 简单的字符计数和计算
- **O(n)**: n = 字符数
- 每条消息 < 1μs

### 压缩开销
- **触发频率**: 仅在超过阈值时
- **I/O操作**: 写入一个归档文件
- **内存**: 临时复制要归档的消息

### 建议阈值

| 使用场景 | threshold | target | keep_recent |
|---------|-----------|--------|-------------|
| 短对话  | 10000     | 8000   | 2000        |
| **标准**(推荐) | 20000 | 16000 | 4000 |
| 长对话  | 30000     | 24000  | 6000        |

## 未来改进

- [ ] 使用LLM生成智能摘要（当前为占位符）
- [ ] 添加读取归档的内置工具
- [ ] 支持多级压缩（压缩摘要本身）
- [ ] 使用tiktoken进行精确token计数
- [ ] 压缩策略优化（按主题、时间段等）

## 相关文件

- `crates/agent-core/src/history.rs` - 压缩实现
- `crates/agent-bot/src/bot.rs` - Bot集成
- `BOT_PERSISTENT_HISTORY.md` - 持久化历史文档
