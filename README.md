# cln-feeder
An automatic fee adjuster for the Bitcoin Lightning Network [CLN](https://github.com/ElementsProject/lightning) Node. 

It tries to optimize fee revenue on a per-channel basis 
by taking a number of past epochs into account to calculate 
new fees for the next epoch. 
It currently only adjusts ppm fee and uses #zerobasefee. 

To run it needs to connect to the CLN RPC Socket. 

## Build from source

```shell
cargo build
```

## Usage

To run feeder needs to connect to the CLN RPC Socket.

```shell
$ cln-feeder --help
cln-feeder 1.0.0

USAGE:
    cln-feeder [OPTIONS] --socket <PATH>

OPTIONS:
    -a, --adjustment-divisor <UINT>    A divisor by which the current fees are divided when an
                                       absolute value must be found to calculate the new fees
                                       [default: 10]
    -d, --data-dir <PATH>              Path to the data directory that feeder uses [default:
                                       ~/.local/cln-feeder/]
    -e, --epochs <EPOCHS>              Past epochs to take into account when calculating new fees
                                       [default: 3]
    -E, --epoch-length <HOURS>         The length of an epoch in hours [default: 24]
    -h, --help                         Print help information
    -l, --log-filter <STRING>          Log Filter [default: cln_feeder]
    -s, --socket <PATH>                Path to the CLN Socket. Usually in
                                       `./clightning/bitcoin/lightning-rpc`
    -t, --temp-database                Use a temporary sqlite database stored in memory
    -v, --verbose                      Log Level
    -V, --version                      Print version information

Process finished with exit code 0

```

## Build and run with Nix/NixOS

This repo has a `flake.nix` with a NixOS module residing in
`nix/modules` to run and configure on NixOS. 