{
  description = "af CLI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      overlay = final: prev: {
        af = final.rustPlatform.buildRustPackage {
          pname = "af";
          version = "0.11.177";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          # Patch octocrab's build.rs to avoid cargo metadata in Nix vendor environment
          postPatch = ''
            cat > $(echo $cargoDepsCopy/octocrab-*/build.rs) << 'EOF'
            use std::env;
            use std::fs;
            use std::path::Path;

            fn main() {
                println!("cargo:rerun-if-changed=Cargo.toml");

                // Generate empty headers array for Nix build
                let array_creation_static = "pub const _SET_HEADERS_MAP: [(&str, &str); 0] = [];";

                let out_dir = env::var("OUT_DIR").unwrap();
                let dest_path = Path::new(&out_dir).join("headers_metadata.rs");
                fs::write(&dest_path, array_creation_static)
                    .expect("failed to write headers_metadata.rs");
            }
            EOF
          '';

          nativeBuildInputs = [
            final.pkg-config
            final.installShellFiles
          ];

          buildInputs = [
            final.openssl
            final.libgit2
            final.libssh2
            final.libiconv
          ];

          postInstall = ''
            installShellCompletion --cmd af \
              --bash <($out/bin/af completions bash) \
              --zsh  <($out/bin/af completions zsh) \
              --fish <($out/bin/af completions fish)
          '';

          meta = {
            description = "Personal helper CLI for dotfiles, git workflows, shortcuts";
            homepage = "https://github.com/smykla-skalski/af";
            license = final.lib.licenses.mit;
            mainProgram = "af";
          };
        };
      };
    in
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ overlay ];
        };
      in {
        packages = {
          default = pkgs.af;
          af = pkgs.af;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.pkg-config
            pkgs.openssl
            pkgs.libgit2
            pkgs.libssh2
            pkgs.libiconv
          ];
        };
      }
    ) // {
      overlays.default = overlay;
    };
}
