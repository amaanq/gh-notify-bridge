# gh-notify-bridge

Poll GitHub notifications and forward to UnifiedPush. No FCM, no Google, no bullshit.

## How it works

```
1. Patched GitHub app registers with UP distributor (ntfy)
2. App gets unique endpoint: https://ntfy.example.com/upABC123
3. App POSTs endpoint to bridge: POST /register {"endpoint": "..."}
4. Bridge polls GitHub every 30s, pushes notifications to that endpoint
5. Your phone receives via UnifiedPush
```

## What you get

- @mentions
- Review requests
- Assignments
- CI/CD activity
- Deployment reviews
- Releases
- Security alerts

## What you don't get

- 2FA push approval (use TOTP or hardware keys instead)
- Copilot live updates

## Usage

```bash
GITHUB_TOKEN="ghp_xxx" ./gh-notify-bridge
```

Then have your app POST to `/register` with the UP endpoint.

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `GITHUB_TOKEN` | Yes | GitHub PAT with `notifications` scope |
| `PORT` | No | HTTP server port (default: 8080) |

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/register` | Register UP endpoint: `{"endpoint": "https://..."}` |
| GET | `/health` | Health check, shows registered endpoint |
| POST | `/poll` | Trigger immediate poll |

## State

Persisted to `state.json`:
- `endpoint` - Registered UnifiedPush endpoint
- `last_poll` - Last poll timestamp (for deduplication)

## NixOS Module

```nix
{
  inputs.gh-notify-bridge.url = "github:amaanq/gh-notify-bridge";

  # In your config:
  services.gh-notify-bridge = {
    enable = true;
    githubTokenFile = "/run/secrets/github-token";
  };
}
```

## Building

```bash
cargo build --release
```

## License

MIT
