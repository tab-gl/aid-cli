# Ace ID CLI

Ace Identity Console — command line interface for Ace Identity.

Manage your AIDC applications and OAuth clients from the terminal.

## Install

```sh
npm install -g ace-id
```

Usage

```sh
aid login
aid whoami
aid apps list
aid apps create --name "My App"
aid clients create --name "Web App" --redirect https://example.com/cb
aid logout
```

Run `aid` with no arguments for the full command list.

## Project setup

Initialize a project with zero unnecessary prompts:

```sh
npx ace-id init
```

Or after installing the package globally:

```sh
npm install -g ace-id
ace-id init
```

The same setup flow is available from both CLI names:

```sh
aid setup
aidc setup
```

`aid` is canonical. `aidc` remains a legacy compatibility alias.


The canonical command is `aid`. The old `aidc` command remains available as a compatibility alias for existing scripts and developer workflows.

Requirements

· Node.js 18 or newer
· A browser (for OIDC login)

License

MIT
