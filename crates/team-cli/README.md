# Team CLI

Interactive command-line tool for testing the `agent-bot` Team multi-bot collaboration system.

## Features

- **Multi-Bot Collaboration**: Create and manage a team of bots
- **Leader Bot**: One bot acts as the leader who coordinates the team
- **Dynamic Bot Creation**: The leader can create new worker bots at runtime
- **Interactive**: Real-time conversation with the team leader
- **Status Tracking**: Monitor team composition and bot count

## Usage

### Basic Usage

```bash
cargo run --package team-cli
```

This starts an interactive session with default settings:
- User name: `Alice`
- Leader name: `LeaderBot`
- Config: `.agent/agent.yaml`

### Command-Line Options

```bash
team-cli [OPTIONS]

Options:
  --user <name>       Set the user name (default: Alice)
  --leader <name>     Set the leader bot name (default: LeaderBot)
  --cfg <path>        Path to config file (default: .agent/agent.yaml)
  --timeout-ms <n>    Timeout in milliseconds (default: 30000)
  -h, --help          Show help message
```

### Interactive Commands

Once running, you can:

- **Send messages**: Type any text and press Enter to send to the leader
- **Check status**: Type `status` to see team information
- **Exit**: Type `exit` to quit

### Example Session

```
$ cargo run --package team-cli

=== Team CLI Ready ===
User: Alice
Leader: LeaderBot
Type messages and press enter. Type 'exit' to quit.
Type 'status' to see team status.

[You -> LeaderBot]: Hello! Can you help me process some data?
[LeaderBot -> You]: Hello Alice! Yes, I can help you with data processing.
                     What kind of data do you need to process?

[You -> LeaderBot]: I need to analyze log files and generate a report
[LeaderBot -> You]: I'll create a specialized worker bot to help with this.

✓ New bot created: LogAnalyzer
  Total bots: 2

[LeaderBot -> You]: I've created a LogAnalyzer bot to handle the log analysis.
                     It will process your files and I'll coordinate the report generation.

status
=== Team Status ===
Total bots: 2
Bots: ["LeaderBot", "LogAnalyzer"]

exit
Shutting down team...
```

## Bot Communication Protocol

Bots communicate using JSON messages:

```json
{
  "to": "recipient_name",
  "content": "message content"
}
```

The leader bot can:
1. Send messages to the user by setting `"to": "Alice"` (or the configured user name)
2. Send messages to worker bots by setting `"to": "<bot_name>"`
3. Receive messages from the user (routed automatically)

Worker bots can:
1. Send messages to other bots (including the leader)
2. **Cannot** send messages directly to the user (only the leader can do this)

## Team Events

The CLI displays these events:

- **UserMessage**: Message from leader to user (displayed as chat)
- **BotCreated**: Notification when a new bot is created
- **Error**: Team errors (e.g., permission violations, unknown bots)

## Architecture

```
User <-> Leader Bot <-> Worker Bots
         |
         v
    Team System
    (routing & coordination)
```

- The **Team** manages all bots and routes messages
- The **Leader** is the only bot that can communicate with the user
- **Worker bots** communicate with each other and report to the leader
- All communication is asynchronous and event-driven

## Configuration

The tool requires an `.agent/agent.yaml` file with OpenAI configuration:

```yaml
model: gpt-4o

openai:
  base_url: https://api.openai.com
  api_key: sk-your-api-key-here
```

## Building

```bash
# Build the CLI
cargo build --package team-cli

# Run with default settings
cargo run --package team-cli

# Run with custom settings
cargo run --package team-cli -- --user Bob --leader CoordinatorBot
```

## Testing

The team-cli is designed for manual testing of the Team system. For automated tests, see `crates/agent-bot/tests/test_team.rs`.

## Future Enhancements

- [ ] Add `@create_bot` command for explicit bot creation
- [ ] Support custom system prompts for leader
- [ ] Add message history view
- [ ] Support bot removal/shutdown
- [ ] Add debugging/trace mode
- [ ] Support multiple teams in one session
