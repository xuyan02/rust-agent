# Disabled Tests

以下测试文件因架构变更而被禁用：

## 被禁用的测试文件

### test_bot.rs.disabled
**原因**: Bot API已完全重构
- 旧API: `Bot::new(runtime, name, agent, sink)` - 允许传入自定义Agent
- 新API: `Bot::new(runtime, name, model, tool_constructors, sink)` - 使用固定的conversation brain + work brain架构
- 测试使用了自定义Agent（ScriptedAgent, BadAgent），无法迁移到新架构

### test_bot_json_protocol.rs.disabled
**原因**: 使用了已删除的`Bot::new_with_session` API
- 测试使用TestAgent验证JSON消息解析
- Bot架构已改为conversation brain + work brain，不再支持直接传入Agent

### test_team.rs.disabled
**原因**: Team API已重构
- 旧API: `Team::new(runtime, user_name, leader_name, leader_agent, sink)` - 允许传入自定义Agent
- 新API: `Team::new(runtime, user_name, leader_name, sink)` - 内部创建Bot
- 测试使用了自定义Agent（EchoAgent, RouterAgent），无法迁移到新架构

### test_team_tools.rs.disabled
**原因**: 同test_team.rs
- Team::new_with_config API已改变
- 不再接受自定义Agent参数

## 当前可用的测试

- ✅ `test_brain.rs` - Brain的基本功能测试（已更新History API）
- ✅ `test_brain_timeout.rs` - Brain超时功能测试（已更新History API和BrainConfig API）

## 建议

这些被禁用的测试需要根据新的Bot/Team架构重写：
1. Bot现在使用conversation brain（对话协调）和work brain（任务执行）
2. 不再允许传入自定义Agent
3. 消息路由通过`@recipient: content`格式和GoalTool实现
4. 需要编写新的集成测试验证conversation brain和work brain的交互
