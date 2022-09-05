{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, crane, flake-utils, ... }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
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
            sqlite
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
      in
      {
        packages.default = cln-feeder;
        checks = {
         inherit
           # Build the crate as part of `nix flake check` for convenience
           cln-feeder;
        };
        nixosModules.default = { pkgs, lib, config, ...}: {
          imports = [
            ./nix/modules/cln-feeder-service.nix
          ];
          nixpkgs.overlays = [ self.overlays.${system}.default ];
        };
        overlays.default = final: prev: {
          ${projectName} = self.packages.${final.hostPlattform.system}.${projectName};
        };
      });
}

