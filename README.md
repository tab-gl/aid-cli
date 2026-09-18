# aidc

Ace Identity Console — command line interface for AIDC.

Manage your AIDC applications and OAuth clients from the terminal.

## Install

```sh
npm install -g ace-id
```

Usage

```sh
aidc login                       # Sign in via Ace ID
aidc whoami                      # Show signed-in account
aidc apps list                   # List applications
aidc apps create --name "My App"
aidc clients create --name "Web App" --redirect https://example.com/cb
aidc logout
```

Run aidc with no arguments for the full command list.

Requirements

· Node.js 18 or newer
· A browser (for OIDC login)

License

MIT
