# Bot PersistentHistory 激活

## 概述

每个Bot的Main Brain现在使用PersistentHistory，自动将对话历史持久化到磁盘。

## 实现细节

### 1. Bot创建时的配置

在`Bot::new()`中：

```rust
// Setup DataStore and create dir_node for this bot
let dir_node = if let Some(data_store) = runtime.data_store() {
    let store = Rc::new(agent_core::DataStore::new(data_store.root().to_path_buf()));
    let bot_dir = store.root_dir().subdir(&name);  // 为每个bot创建子目录
    Some(bot_dir)
} else {
    None
};

// Create Main Brain Session with PersistentHistory
let mut main_brain_builder = SessionBuilder::new(runtime)
    .set_default_model(model)
    // ... 添加tools ...

// Set dir_node for persistent storage
if let Some(dir_node) = dir_node {
    main_brain_builder = main_brain_builder.set_dir_node(dir_node);
}

// Use PersistentHistory for Main Brain
main_brain_builder = main_brain_builder
    .set_history(Box::new(agent_core::PersistentHistory::new()));
```

### 2. 存储路径结构

```
<data_store_root>/
  <bot_name>/
    history.yaml  <- Bot的对话历史
```

**示例：**
- data_store_root = `/workspace/.agent`
- bot_name = `alice`
- 历史文件 = `/workspace/.agent/alice/history.yaml`

### 3. Runtime配置

在创建Runtime时需要设置data_store_root：

```rust
let runtime = RuntimeBuilder::new()
    .set_data_store_root(workspace_path.join(".agent"))
    .build();
```

**推荐配置：**
- 开发环境：`<workspace>/.agent`
- 测试环境：`<temp_dir>`

### 4. 历史记录格式

存储在YAML文件中的格式：

```yaml
type_tag: agent_core::history::HistoryData
value:
  messages:
  - role: User
    content: !Text "@alice: Hello"
  - role: Assistant
    content: !Text "@user: Hi there!"
```

## 特性

### ✅ 自动持久化
- 每次`append()`自动flush到磁盘
- 无需手动保存

### ✅ 隔离存储
- 每个Bot有独立的存储目录
- 互不干扰

### ✅ 类型安全
- 使用TypeInfo进行运行时类型验证
- 防止类型错误

### ✅ 滑动窗口
- 默认最大1000条消息
- 自动清理旧消息

### ✅ Deep Brain使用内存历史
- Deep Brain（ReActAgent）使用InMemoryHistory
- 每次调用都是独立的上下文
- 不污染Main Brain的持久化历史

## 使用示例

### 创建带持久化历史的Bot

```rust
use agent_bot::Bot;
use agent_core::RuntimeBuilder;
use std::rc::Rc;

// 1. 创建Runtime with DataStore
let runtime = Rc::new(
    RuntimeBuilder::new()
        .set_local_spawner(spawner)
        .set_data_store_root(workspace.join(".agent"))  // 设置存储根目录
        .build()
);

// 2. 创建Bot（自动使用PersistentHistory）
let bot = Bot::new(
    runtime,
    "alice",                    // Bot名称
    "gpt-4o",                   // 模型
    tool_constructors,          // 工具构造器
    event_sink,                 // 事件接收器
)?;

// 3. Bot的历史会自动保存到: <data_store_root>/alice/history.yaml
```

### 多个Bot共存

```rust
// Bot1 - 历史保存到 <data_store_root>/bot1/history.yaml
let bot1 = Bot::new(runtime.clone(), "bot1", "gpt-4o", tools, sink1)?;

// Bot2 - 历史保存到 <data_store_root>/bot2/history.yaml
let bot2 = Bot::new(runtime.clone(), "bot2", "gpt-4o", tools, sink2)?;

// 每个Bot有独立的历史记录，互不干扰
```

## 测试

### 基本测试

```rust
#[tokio::test]
async fn test_bot_uses_persistent_history() {
    let runtime = RuntimeBuilder::new()
        .set_data_store_root(temp_dir.path().to_path_buf())
        .build();

    let bot = Bot::new(runtime, "test_bot", "gpt-4o", tools, sink)?;

    // 验证历史路径
    let expected = temp_dir.path().join("test_bot").join("history.yaml");
    // ... 验证逻辑 ...
}
```

运行测试：
```bash
cargo test -p agent-tests --test test_bot_persistent_history
```

## 与旧设计的对比

### 旧设计（InMemoryHistory）
- ❌ 对话不持久化
- ❌ Bot重启后丢失历史
- ❌ 无法恢复对话上下文

### 新设计（PersistentHistory）
- ✅ 对话自动持久化到磁盘
- ✅ Bot重启后保留历史
- ✅ 可以恢复对话上下文
- ✅ 支持长期对话
- ✅ 每个Bot独立存储

## 路径配置说明

### DataStore vs Session路径

**DataStore路径（新）：**
- 用于数据持久化（历史记录、状态等）
- 通过`dir_node`配置
- 路径：`<data_store_root>/<bot_name>/`

**Session路径（保留）：**
- `workspace_path`：用于文件工具的相对路径解析
- `agent_path`：用于工具输出的spool目录
- 这些路径仍然有用，不删除

### 为什么保留Session路径？

1. **workspace_path**：
   - file-read、file-glob等工具需要它来解析相对路径
   - 例如：`file-read src/main.rs` → `<workspace_path>/src/main.rs`

2. **agent_path**：
   - 用于存储工具的大输出文件（spool）
   - 例如：`<agent_path>/spool/123456_shell-exec.log`

3. **dir_node（新增）**：
   - 用于Bot自己的数据存储（历史记录、状态等）
   - 例如：`<data_store_root>/<bot_name>/history.yaml`

这三个路径服务于不同的目的，都是必要的。

## 配置建议

### 生产环境
```rust
RuntimeBuilder::new()
    .set_data_store_root(workspace.join(".agent"))
    .build()
```

### 测试环境
```rust
RuntimeBuilder::new()
    .set_data_store_root(temp_dir.path().to_path_buf())
    .build()
```

## 故障排查

### 问题：历史没有保存

**检查：**
1. Runtime是否设置了data_store_root？
2. 目录是否有写权限？
3. 磁盘空间是否充足？

### 问题：无法加载历史

**检查：**
1. YAML文件格式是否正确？
2. type_tag是否匹配？
3. 文件是否被手动修改？

### 问题：历史文件太大

**解决方案：**
```rust
// 使用自定义的滑动窗口大小
let history = PersistentHistory::new_with_limit(500);  // 只保留500条
```

## 未来改进

可能的优化方向：
- [ ] 历史记录压缩
- [ ] 分页加载大历史
- [ ] 历史记录备份
- [ ] 历史记录搜索索引
- [ ] 历史记录加密

## 相关文件

- `crates/agent-bot/src/bot.rs` - Bot实现，配置PersistentHistory
- `crates/agent-core/src/history.rs` - PersistentHistory实现
- `crates/agent-core/src/data_store.rs` - DataStore实现
- `crates/agent-tests/tests/test_bot_persistent_history.rs` - 测试
- `PERSISTENT_HISTORY.md` - PersistentHistory详细文档
