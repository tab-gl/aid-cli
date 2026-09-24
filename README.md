# aid

Ace Identity Console — command line interface for Ace Identity.

Manage your AIDC applications and OAuth clients from the terminal.

## Install

```sh
npm install -g ace-id
```

Usage

\`\`\`sh
aid login
aid whoami
aid apps list
aid apps create --name "My App"
aid clients create --name "Web App" --redirect https://example.com/cb
aid logout
\`\`\`

Run `aid` with no arguments for the full command list.


The canonical command is `aid`. The old `aidc` command remains available as a compatibility alias for existing scripts and developer workflows.

Requirements

· Node.js 18 or newer
· A browser (for OIDC login)

License

MIT
