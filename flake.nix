{
  description = "A process I/O multiplexer";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/staging-next";

  inputs.import-cargo.url = github:edolstra/import-cargo;

  outputs = { self, nixpkgs, import-cargo }:
    let

      # System types to support.
      supportedSystems = [ "x86_64-linux" ];

      # Helper function to generate an attrset '{ x86_64-linux = f "x86_64-linux"; ... }'.
      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: f system);

      # Nixpkgs instantiated for supported system types.
      nixpkgsFor = forAllSystems (system: import nixpkgs { inherit system; overlays = [ self.overlay ]; });

    in {

      # A Nixpkgs overlay.
      overlay = final: prev: {

        piomux = with final; final.callPackage ({ inShell ? false }: stdenv.mkDerivation rec {
          pname = "piomux";
          version = "0.1.0";

          # In 'nix develop', we don't need a copy of the source tree
          # in the Nix store.
          src = if inShell then null else ./.;

          buildInputs =
            [ rustc
              cargo
            ] ++ (if inShell then [
              # In 'nix develop', provide some developer tools.
              rustfmt
              clippy
            ] else [
              (import-cargo.builders.importCargo {
                lockFile = ./Cargo.lock;
                inherit pkgs;
              }).cargoHome
            ]);

          target = "--release";

          buildPhase = "cargo build ${target} --frozen --offline";

          doCheck = true;

          checkPhase = "cargo test ${target} --frozen --offline";

          installPhase =
            ''
              mkdir -p $out
              cargo install --frozen --offline --path . --root $out
              rm $out/.crates.toml
            '';
        }) {};

      };

      # Provide some binary packages for selected system types.
      packages = forAllSystems (system:
        {
          inherit (nixpkgsFor.${system}) piomux;
        });

      # The default package for 'nix build'. This makes sense if the
      # flake provides only one package or there is a clear "main"
      # package.
      defaultPackage = forAllSystems (system: self.packages.${system}.piomux);

      # Provide a 'nix develop' environment for interactive hacking.
      devShell = forAllSystems (system: self.packages.${system}.piomux.override { inShell = true; });

      # Tests run by 'nix flake check' and by Hydra.
      checks = forAllSystems
        (system:
          with nixpkgsFor.${system};
          {
            inherit (self.packages.${system}) piomux;
          }
        );
    };
}
