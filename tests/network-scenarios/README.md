# Network Testing Scenarios

Test AgentBeam's P2P capabilities across different network configurations.

## Prerequisites

- Docker and Docker Compose installed
- AgentBeam built with `cargo build --release`

## Test Scenarios

### 1. Direct Connection Test

Tests direct P2P connection when both peers are on the same network:

```bash
docker-compose -f docker-compose.direct.yml up
```

Expected: Connection should use "direct" path (no relay).

### 2. Relay Connection Test

Tests relay fallback when peers are on isolated networks:

```bash
docker-compose -f docker-compose.relay.yml up
```

Expected: Connection should use "relay" path.

## Validation

Check logs for connection type:
- Look for: `event="connection_established" path="direct"` or `path="relay"`
- Direct mode should show significantly faster transfer speeds
- Relay mode should still complete successfully but may be slower

## Future Tests

- [ ] NAT traversal simulation
- [ ] Network interruption recovery
- [ ] Multi-peer swarm testing
- [ ] Bandwidth-limited scenarios