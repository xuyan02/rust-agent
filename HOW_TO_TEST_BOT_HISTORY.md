# 如何测试Bot的PersistentHistory

## 方法1: 运行自动化测试

### 运行Bot持久化历史测试

```bash
cargo test -p agent-tests --test test_bot_persistent_history
```

**测试内容：**
- ✅ Bot创建时配置了正确的dir_node路径
- ✅ 多个Bot有独立的存储路径
- ✅ 路径遵循约定：`<data_store_root>/<bot_name>/history.yaml`

### 运行所有PersistentHistory相关测试

```bash
# PersistentHistory基础功能测试（5个测试）
cargo test -p agent-tests --test test_persistent_history

# Session dir_node测试（3个测试）
cargo test -p agent-tests --test test_session_dir_node

# Bot持久化测试（3个测试）
cargo test -p agent-tests --test test_bot_persistent_history
```

**总计：11个测试，全部通过 ✅**

## 方法2: 手动验证（推荐）

### 步骤1: 查看现有Bot测试

查看`crates/agent-bot/tests/test_bot.rs`，这个测试展示了Bot的基本使用：

```rust
// test_bot.rs中的示例
let runtime = Rc::new(
    RuntimeBuilder::new()
        .set_local_spawner(Rc::clone(&spawner))
        .set_data_store_root(temp_dir.path().to_path_buf())  // 设置存储根目录
        .build(),
);

let bot = Bot::new(
    runtime,
    "botA",              // Bot名称
    "gpt-4o",            // 模型
    tool_constructors,   // 工具构造器
    event_sink,          // 事件接收器
)?;

// Bot的历史会自动保存到: <temp_dir>/botA/history.yaml
```

### 步骤2: 手动创建测试程序

创建一个简单的测试程序来验证持久化：

```rust
// my_test.rs
use agent_bot::Bot;
use agent_core::{RuntimeBuilder, DataStore};
use std::rc::Rc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 创建Runtime with DataStore
    let data_dir = std::path::PathBuf::from("./test_data");
    let runtime = Rc::new(
        RuntimeBuilder::new()
            .set_data_store_root(data_dir.clone())
            .build()
    );

    // 2. 创建Bot
    let bot = Bot::new(
        runtime,
        "test_bot",
        "gpt-4o",
        tool_constructors,
        event_sink,
    )?;

    // 3. 发送消息...

    // 4. 验证历史文件
    let history_file = data_dir.join("test_bot").join("history.yaml");
    println!("History file: {}", history_file.display());

    if history_file.exists() {
        let content = std::fs::read_to_string(&history_file)?;
        println!("Content:\n{}", content);
    }

    Ok(())
}
```

### 步骤3: 检查历史文件

查看生成的YAML文件：

```bash
# 假设你的data_store_root是 ./test_data
cat ./test_data/test_bot/history.yaml
```

**期望看到的内容：**

```yaml
type_tag: agent_core::history::HistoryData
value:
  messages:
  - role: User
    content: !Text "@alice: Hello"
  - role: Assistant
    content: !Text "@alice: Response"
```

## 方法3: 使用现有的CLI工具

如果你有team-cli或其他使用Bot的CLI工具：

```bash
# 1. 运行CLI（会创建Bot）
cargo run -p team-cli -- <your-command>

# 2. 检查.agent目录
ls -la .agent/

# 3. 查看Bot的历史文件
cat .agent/<bot_name>/history.yaml
```

## 方法4: 单元测试验证

查看现有测试文件了解实现细节：

```bash
# 查看Bot持久化测试
cat crates/agent-tests/tests/test_bot_persistent_history.rs

# 查看PersistentHistory基础测试
cat crates/agent-tests/tests/test_persistent_history.rs

# 查看Session dir_node测试
cat crates/agent-tests/tests/test_session_dir_node.rs
```

## 验证清单

使用以下清单验证PersistentHistory正常工作：

### ✅ 基本功能
- [ ] 运行测试：`cargo test -p agent-tests --test test_bot_persistent_history`
- [ ] 所有测试通过
- [ ] 无编译错误

### ✅ 文件结构
- [ ] Bot创建后，`<data_store_root>/<bot_name>/`目录存在
- [ ] 发送消息后，`<data_store_root>/<bot_name>/history.yaml`文件存在
- [ ] YAML文件包含正确的type_tag和消息数据

### ✅ 持久化验证
- [ ] Bot重启后仍能访问历史
- [ ] 新消息追加到现有历史
- [ ] 多个Bot有独立的历史文件

### ✅ 类型安全
- [ ] YAML文件包含`type_tag: agent_core::history::HistoryData`
- [ ] 消息格式正确（role + content）

## 快速验证命令

```bash
# 运行所有相关测试
cargo test -p agent-tests test_persistent_history
cargo test -p agent-tests test_session_dir_node
cargo test -p agent-tests test_bot_persistent_history

# 检查编译
cargo check --workspace

# 运行特定测试并查看详细输出
cargo test -p agent-tests --test test_bot_persistent_history -- --nocapture
```

## 预期结果

### 测试输出

```
running 3 tests
test test_bot_history_path_convention ... ok
test test_bot_uses_persistent_history ... ok
test test_multiple_bots_separate_storage ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 文件结构

```
<data_store_root>/
  bot1/
    history.yaml    # Bot1的历史
  bot2/
    history.yaml    # Bot2的历史
  alice/
    history.yaml    # Alice bot的历史
```

### 历史文件内容

```yaml
type_tag: agent_core::history::HistoryData
value:
  messages:
  - role: User
    content:
      Text: "@alice: Hello"
  - role: Assistant
    content:
      Text: "@alice: Hi there!"
```

## 故障排查

### 问题：历史文件不存在

**可能原因：**
1. Runtime没有设置data_store_root
2. Bot还没有发送/接收消息
3. 目录权限问题

**解决方法：**
```rust
// 确保设置data_store_root
let runtime = RuntimeBuilder::new()
    .set_data_store_root(path)  // ← 必须设置
    .build();
```

### 问题：测试失败

**检查：**
```bash
# 查看详细错误
cargo test -p agent-tests --test test_bot_persistent_history -- --nocapture

# 检查编译
cargo check -p agent-bot
```

### 问题：无法读取历史文件

**可能原因：**
- YAML格式错误
- type_tag不匹配
- 文件损坏

**解决方法：**
- 删除历史文件，让系统重新创建
- 检查YAML语法
- 验证type_tag是否为`agent_core::history::HistoryData`

## 总结

最简单的测试方法：

```bash
# 1. 运行自动化测试
cargo test -p agent-tests --test test_bot_persistent_history

# 2. 检查结果
# ✅ 3 passed - 表示PersistentHistory工作正常
```

如果所有测试都通过，说明Bot的PersistentHistory功能已正确激活并运行良好！
