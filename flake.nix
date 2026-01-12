{
  description = "Poll GitHub notifications and forward to UnifiedPush/ntfy";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = self.packages.${system}.gh-notify-bridge;
          gh-notify-bridge = pkgs.rustPlatform.buildRustPackage {
            pname = "gh-notify-bridge";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            meta = {
              description = "Poll GitHub notifications and forward to UnifiedPush/ntfy";
              homepage = "https://github.com/amaanq/gh-notify-bridge";
              license = pkgs.lib.licenses.mit;
              mainProgram = "gh-notify-bridge";
            };
          };
        }
      );

      nixosModules.default = import ./module.nix self;

      overlays.default = final: prev: {
        gh-notify-bridge = self.packages.${prev.system}.gh-notify-bridge;
      };
    };
}
