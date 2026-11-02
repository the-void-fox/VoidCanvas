{
  description = "Void Canvas — a terminal-aesthetic ASCII art editor";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      # winit and wgpu are only wired up for Linux here.
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      package =
        { lib
        , rustPlatform
        , makeDesktopItem
        , copyDesktopItems
        , libGL
        , vulkan-loader
        , wayland
        , libxkbcommon
        , libx11
        , libxcb
        , libxcursor
        , libxi
        , libxrandr
        }:
        let
          # winit and wgpu dlopen these instead of linking them,
          # so nothing puts them on the binary's RUNPATH by itself.
          runtimeLibs = [ libGL vulkan-loader wayland libxkbcommon libx11 libxcb libxcursor libxi libxrandr ];
        in
        rustPlatform.buildRustPackage {
          pname = "void-canvas";
          version = (lib.importTOML ./Cargo.toml).package.version;

          src = lib.fileset.toSource {
            root = ./.;
            # examples/ is read by the tests.
            fileset = lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./src ./examples ];
          };
          cargoLock.lockFile = ./Cargo.lock;

          # Release tests recompile everything with LTO and nearly double the build;
          # they run in `nix flake check` (and in CI) instead.
          doCheck = false;

          nativeBuildInputs = [ copyDesktopItems ];

          desktopItems = [
            (makeDesktopItem {
              name = "void-canvas";
              desktopName = "Void Canvas";
              comment = "Terminal-aesthetic ASCII art editor";
              exec = "void_canvas %f";
              icon = "void-canvas";
              categories = [ "Graphics" "2DGraphics" ];
              # The app_id set in main.rs.
              startupWMClass = "void-canvas";
            })
          ];

          postInstall = ''
            install -Dm644 src/assets/icon.png $out/share/icons/hicolor/32x32/apps/void-canvas.png
          '';

          # After fixupPhase, whose --shrink-rpath would drop the unlinked libraries again.
          postFixup = ''
            patchelf --add-rpath ${lib.makeLibraryPath runtimeLibs} $out/bin/void_canvas
          '';

          meta = {
            description = "Terminal-aesthetic ASCII art editor";
            homepage = "https://github.com/the-void-fox/VoidCanvas";
            license = lib.licenses.gpl3Plus;
            mainProgram = "void_canvas";
            platforms = lib.platforms.linux;
          };
        };
    in
    {
      overlays.default = final: prev: {
        void-canvas = final.callPackage package { };
      };

      packages = forAllSystems (pkgs: rec {
        void-canvas = pkgs.callPackage package { };
        default = void-canvas;
      });

      checks = forAllSystems (pkgs: {
        tests = self.packages.${pkgs.stdenv.hostPlatform.system}.void-canvas.overrideAttrs { doCheck = true; };
      });

      # The same rustup-based shell as `nix-shell`.
      devShells = forAllSystems (pkgs: {
        default = import ./shell.nix { inherit pkgs; };
      });
    };
}
