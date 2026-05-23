{
  description = "ijevim - A minimal Vim-like TUI editor written in Rust";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    let
      rustVersion = "1.80.0";
    in
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells = {
          default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              pkg-config
              openssl
              llvmPackages.clang
              rustfmt
              clippy
            ];

            RUST_BACKTRACE = "1";

            shellHook = ''
              echo "═══════════════════════════════════════════════════"
              echo "  ijevim development environment (Rust ${rustVersion})"
              echo "═══════════════════════════════════════════════════"
              echo "  Build:   cargo build --release"
              echo "  Run:     cargo run -- [file]"
              echo "  Test:    cargo test"
              echo "  Clippy:  cargo clippy"
              echo "  Format:  cargo fmt"
              echo "═══════════════════════════════════════════════════"
            '';
          };
        };
      }
    );
}
