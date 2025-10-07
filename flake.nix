{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = inputs@{self, ...}: inputs.flake-utils.lib.eachDefaultSystem (system: let
    pkgs = import inputs.nixpkgs {inherit system;};
    craneLib = inputs.crane.mkLib pkgs;

    commonArgs = {
      src = craneLib.cleanCargoSource ./.;
      strictDeps = true;

      buildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
        pkgs.libiconv
      ];
    };

    wrun = craneLib.buildPackage (commonArgs // {
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    });
  in {
    checks = {
      inherit wrun;
    };

    packages.default = wrun;

    devShells.default = craneLib.devShell {
      checks = self.checks.${system};
    };
  });
}
