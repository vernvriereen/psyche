# psyche

Psyche uses `just` to run some common tasks!
It uses `nix` as a build system, to make your life easier.
To install `nix`, simply run `curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install` or find it at your local package manager.

## Lints & Checks

`$ just check`

If it passes, CI will pass.

## Formatting

`$ just fmt`

## Building

You can build individual binaries with commands like

```bash
nix build .#psyche-centralized-client
nix build .#psyche-centralized-server
nix build .#expand-distro
```

## Building & pushing Docker images

To build the centralized client & push it to docker.io's hub,
`$ just docker-push-centralized-client`

## Utils

### compare-hf-psyche.sh

compares hf & psyche training implementations bit-for-bit.
