{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, crane, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        projectName = "cln-feeder";

        craneLib = crane.lib.${system};

        # Common derivation arguments used for all builds
        commonArgs = {
          src = ./.;
          pname = projectName;
          version = "1.0.0";

          buildInputs = with pkgs; [
            # Add extra build inputs here, etc.
            protobuf
            openssl
            perl
            rustfmt
          ];

          nativeBuildInputs = with pkgs; [
            # Add extra native build inputs here, etc.
            pkg-config
          ];
        };

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
          # Additional arguments specific to this derivation can be added here.
          # Be warned that using `//` will not do a deep copy of nested
          # structures
          # pname = "projectName";
        });

        # Run clippy (and deny all warnings) on the crate source,
        # resuing the dependency artifacts (e.g. from build scripts or
        # proc-macros) from above.
        #
        # Note that this is done as a separate derivation so it
        # does not impact building just the crate by itself.
        cln-feederClippy = craneLib.cargoClippy (commonArgs // {
          # Again we apply some extra arguments only to this derivation
          # and not every where else. In this case we add some clippy flags
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        });

        # Build the actual crate itself, reusing the dependency
        # artifacts from above.
        cln-feeder = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });

        # Also run the crate tests under cargo-tarpaulin so that we can keep
        # track of code coverage
        cln-feederCrateCoverage = craneLib.cargoTarpaulin (commonArgs // {
          inherit cargoArtifacts;
        });
      in
      {
        packages.default = cln-feeder;
        checks = {
         inherit
           # Build the crate as part of `nix flake check` for convenience
           cln-feeder
           cln-feederClippy
           cln-feederCrateCoverage;
        };
#        nixosModules.default = { pkgs, lib, config, ...}: {
#          imports = [
#            ./nix/modules/watchtower-plugin.nix
#            ./nix/modules/teos-service.nix
#          ];
#          nixpkgs.overlays = [ self.overlays.default ];
#        };
#        overlays.default = final: prev: {
#          ${projectName} = self.packages.${final.hostPlattform.system}.${projectName};
#        };
      });
}

