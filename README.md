# AgentBeam

P2P workspace and session sharing for Claude Code developers.

## P2P for Agents (Claude Code initially)

AgentBeam enables direct computer-to-computer transfer of complete Claude Code working states including conversation context, memory files, and exact codebase state. No servers, no accounts, no uploads - just direct P2P transfer using the Iroh framework.

### How P2P Transfer Works

**Default Mode: Direct Connection (Default)**: AgentBeam establishes encrypted peer-to-peer connections directly between your devices using Iroh's QUIC protocol. This provides:

- **True P2P**: Only you and the recipient - no intermediary servers see your data
- **NAT Traversal**: Automatic hole punching through firewalls and routers (~90% success rate)
- **End-to-End Encryption**: Data encrypted specifically for the destination device
- **Optimal Performance**: Direct paths provide lowest latency and highest speeds

<details>
<summary><strong>When Direct P2P Might Not Work</strong></summary>

Direct connections can fail when both devices are behind restrictive NATs or corporate firewalls that block hole punching. In these cases:

- **Corporate Networks**: Some enterprise firewalls block UDP hole punching
- **Symmetric NATs**: Some router configurations prevent direct connection establishment
- **Restricted Networks**: Networks that only allow HTTP/HTTPS traffic
- **Geographic Distance**: Very distant peers may have routing issues

**Solution**: Use relay mode (see below) or connect to the same WiFi network.

</details>

**Relay Fallback**: When direct connections aren't possible, Iroh seamlessly falls back to relay servers that:

- **Coordinate Connections**: Help establish the initial encrypted tunnel
- **Route Encrypted Traffic**: Cannot decrypt your data (end-to-end encrypted)
- **Step Back Automatically**: Once direct connection succeeds, relay stops routing traffic
- **Ensure Reliability**: ~100% connection success rate across any network configuration

**Discovery (Roadmap)**: Future versions may include automatic peer discovery to:

- **Find Local Peers**: Automatically detect AgentBeam users on your network
- **NodeID-Only Sharing**: Connect using short IDs instead of long tickets
- **Agent Swarms**: Enable multi-agent collaboration and capability sharing

## Features

- **Direct P2P Transfer**: Share workspaces directly between machines
- **Smart File Filtering**: Respects `.gitignore` and `.beamignore` patterns
- **Memory-Safe Streaming**: Handles large workspaces (5GB+) with minimal RAM usage
- **Provider Monitoring**: Sender knows when transfer is complete
- **Automatic Resume**: Interrupted transfers can be resumed
- **Test Mode**: Safe testing with dummy data

## Installation

```bash
# Clone and build from source
git clone https://github.com/agentbeam/agentbeam
cd agentbeam
cargo build --release

# Install to PATH
cargo install --path .
```

## Usage

### Sharing a Workspace

```bash
# Share current directory
agentbeam beam-session

# Test mode with dummy data
agentbeam beam-session --test-mode

# Custom workspace path
agentbeam beam-session --workspace /path/to/project

# Direct P2P only (no relay)
agentbeam beam-session --no-relay
```

The command will:

1. Package your workspace (respecting ignore files)
2. Generate a sharing ticket
3. Wait for recipient to connect
4. Show transfer progress
5. Notify when transfer is complete

### Receiving a Workspace

```bash
# Receive to default directory (./beamed-workspace)
agentbeam receive <ticket>

# Specify target directory
agentbeam receive <ticket> --target /path/to/destination
```

## Ignore Patterns

AgentBeam respects the following ignore patterns in order:

1. `.beamignore` - Custom patterns for beaming
2. `.gitignore` - Standard git ignore patterns
3. Default excludes (node_modules/, target/, .env, etc.)

Example `.beamignore`:

```
*.secret
*.key
credentials.json
large_data/
```

## Architecture

- **Iroh Framework**: P2P networking with QUIC protocol
- **Collections**: Native Iroh format for multi-file transfer
- **FsStore**: Disk-based blob storage (no memory issues)
- **Provider Events**: Real-time transfer monitoring
- **RAII Cleanup**: Automatic temp directory cleanup

## Technical Details

- Max workspace size: 5GB (configurable with `--force`)
- Warning threshold: 1GB
- Protocol: QUIC with optional relay
- Storage: Temporary `.agentbeam-*` directories (auto-cleaned)

## Development Roadmap

### ✅ MVP Phase 1: Core Infrastructure (COMPLETED)

- [x] **Iroh Integration**: P2P endpoint setup with FsStore
- [x] **Collection-Based Transfer**: Native Iroh Collections (not TAR)
- [x] **Provider Event Monitoring**: Real-time upload progress tracking
- [x] **Memory-Safe Operations**: FsStore with automatic cleanup (RAII)
- [x] **Basic CLI Interface**: `beam-session` and `receive` commands
- [x] **Connection Modes**: Direct P2P, relay support, custom relay URLs
- [x] **Progress Bars**: Visual feedback for file collection and transfers
- [x] **Metadata Storage**: Session metadata embedded in collections

### ✅ MVP Phase 2: Workspace Integration (COMPLETED)

- [x] **Gitignore Support**: Uses `ignore` crate for `.gitignore` and `.beamignore`
- [x] **File Filtering**: Respects ignore patterns, excludes sensitive files
- [x] **Dummy Workspace Generator**: Safe testing with realistic test data
- [x] **Test Mode**: `--test-mode` flag for safe development testing
- [x] **Size Validation**: Configurable limits with override options
- [x] **Cross-Platform Paths**: Proper path handling for different OSes

### ✅ MVP Phase 3: Polish & Robustness (COMPLETED)

- [x] **Complete Progress Tracking**: Both sender and receiver progress bars
- [x] **Transfer Completion Detection**: Provider knows when safe to close
- [x] **Resume Support**: Built-in via Iroh's partial download capabilities
- [x] **Error Handling**: Clear messages for connection failures, size limits
- [x] **Resource Cleanup**: Temp directories cleaned up on exit/panic
- [x] **User Confirmation**: Safety prompts before sharing real data

### 🚧 Pre-Release Phase: Production Readiness

- [x] **Real Claude Code Integration**: Remove test-mode requirement
  - [x] Detect active Claude Code sessions
  - [x] Package conversation history and memory files
  - [x] Integrate with Claude Code workspace detection
- [x] **Session Restoration**: Proper unpacking of Claude Code state
  - [x] Restore conversation context to Claude Code
  - [x] Handle session file placement correctly
  - [x] Git state restoration (branch, uncommitted changes)
- [ ] **Discovery Integration**: Enable automatic peer finding
  - [ ] DNS Discovery for NodeID-based connections
  - [ ] Local Network Discovery for same-network peers
  - [ ] Short sharing codes instead of long tickets
- [ ] **Security Hardening**:
  - [ ] Secret scanning and warnings
  - [ ] Network security validation
  - [ ] File permission preservation
- [ ] **UX Polish**:
  - [ ] Better error messages and troubleshooting
  - [ ] Connection diagnostics
  - [ ] Transfer speed optimization

### 🎯 Post-Launch: Advanced Features

- [ ] **MCP Integration**: Tool for Claude Code to call beam commands
- [ ] **Background Daemon**: Long-running service for instant sharing
- [ ] **Auto-Discovery**: Find peers on same network automatically
- [ ] **Session History**: Browse and restore previous beam transfers
- [ ] **Multi-Beam**: Send to multiple recipients simultaneously
- [ ] **Compression**: Optional compression for large workspaces
- [ ] **Encryption**: End-to-end encryption beyond QUIC

## Testing

```bash
# Run tests
cargo test

# Test with dummy data
cargo run -- beam-session --test-mode

# Clean test data
cargo run -- cleanup-test
```

## Development

The codebase is organized as:

- `src/core/` - Core P2P and transfer logic
- `src/cli/` - Command-line interface
- `src/test_utils/` - Testing utilities and dummy data generation

Key components:

- `AgentBeam` - Main struct managing Iroh endpoint and blob storage
- `FileCollector` - Handles workspace file collection and bundling
- `ProviderMonitor` - Tracks upload progress and transfer completion
- `Receiver` - Manages downloads with resume support
