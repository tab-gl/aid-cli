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

Fast setup uses sensible defaults with no wizard prompts:

```sh
npx ace-id init
```

Or after installing the package globally:

```sh
npm install -g ace-id
ace-id init
```

The same setup flow is available from all CLI aliases:

```sh
aid setup
ace-id setup
aidc setup
```

For the interactive wizard:

```sh
aid setup --interactive
# or
ace-id setup --wizard
```

The wizard lets you review or change the project name, redirect URI, and scopes before anything is created.

`aid` is canonical. `ace-id` and `aidc` remain compatibility aliases, and all three share the same command behavior.

Requirements

· Node.js 18 or newer
· A browser (for OIDC login)

License

MIT
