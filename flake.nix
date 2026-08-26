{
  description = "QBZ — Native hi-fi Qobuz desktop player for Linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        # ──────────────────────────────────────────────
        # VERSION BUMP: update version, rev, and srcHash
        # when tagging a new release. (npmHash is gone —
        # v2.0+ is the Slint crates/ workspace, no node.)
        # ──────────────────────────────────────────────
        qbzVersion = "2.0.2";
        qbzRev     = "v${qbzVersion}";
        srcHash    = "sha256-zseGL7IcH/fdc4TDVwU3Tml1X6wCvSaYCji5D5RxAuA=";

        # Runtime libraries winit/wgpu/glutin dlopen at runtime — a Nix
        # binary cannot find system copies, so the installed program is
        # wrapped with this library path.
        runtimeLibs = with pkgs; [
          wayland
          libxkbcommon
          libglvnd
          vulkan-loader
          # X11 session support: winit dlopens these when not on Wayland
          # (review fix — without them the app is Wayland-only on Nix).
          xorg.libX11
          xorg.libXcursor
          xorg.libXi
        ];
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage rec {
          pname = "qbz";
          version = qbzVersion;

          src = pkgs.fetchFromGitHub {
            owner = "vicrodh";
            repo  = "qbz";
            rev   = qbzRev;
            hash  = srcHash;
          };

          cargoRoot = "crates";
          buildAndTestSubdir = cargoRoot;
          # Build only the app binary, not every workspace member's tests.
          cargoBuildFlags = [ "-p" "qbz-qt" ];

          cargoLock = {
            lockFile = "${src}/crates/Cargo.lock";
          };

          env.LIBCLANG_PATH = "${pkgs.lib.getLib pkgs.llvmPackages.libclang}/lib";

          # Qt frontend (2026-08-25): cxx-qt needs qtbase + qtdeclarative with
          # their private headers (the RHI scene-graph items include
          # <rhi/qrhi.h>); qtshadertools provides `qsb` (optional — the .qsb
          # are committed); qtwayland is the Wayland platform plugin.
          # wrapQtAppsHook sets the plugin/QML paths the binary needs at run
          # time. nixpkgs' qt6 (6.9) is above the 6.8 floor.
          nativeBuildInputs = with pkgs; [
            clang
            pkg-config
            cmake
            nasm
            qt6.wrapQtAppsHook
            qt6.qtshadertools
          ];

          buildInputs = with pkgs; [
            alsa-lib
            fontconfig
            freetype
            libjack2
            dbus
            openssl
            qt6.qtbase
            qt6.qtdeclarative
            qt6.qtwayland
          ];

          # Tests need an offscreen QPA + D-Bus the sandbox does not have;
          # the crates are tested in the repo's CI (test-crates.yml).
          doCheck = false;

          postInstall = ''
            # wrapQtAppsHook wraps $out/bin/qbz; only the non-Qt dlopen'd
            # backends need the extra library path.
            qtWrapperArgs+=(--prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibs})

            install -Dm644 $src/packaging/linux/qbz.desktop \
              $out/share/applications/qbz.desktop
            for size in 32 48 64 128 256 512; do
              install -Dm644 $src/packaging/icons/"$size"x"$size".png \
                $out/share/icons/hicolor/"$size"x"$size"/apps/qbz.png
            done
          '';

          meta = with pkgs.lib; {
            description = "Native, full-featured hi-fi Qobuz desktop player for Linux";
            homepage = "https://qbz.lol";
            license = licenses.mit;
            mainProgram = "qbz";
            platforms = platforms.linux;
          };
        };

        # Dev shell with all build dependencies
        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];

          # `inputsFrom` pulls in buildInputs/nativeBuildInputs from the
          # package but does NOT propagate `env.*` attributes, so we must
          # re-export LIBCLANG_PATH here for bindgen.
          LIBCLANG_PATH = "${pkgs.lib.getLib pkgs.llvmPackages.libclang}/lib";

          packages = with pkgs; [
            rust-analyzer
            rustfmt
            clippy
          ];

          # The package's `postInstall` wraps the installed binary with
          # LD_LIBRARY_PATH for the dlopen'd display/GPU stack. Inside
          # `nix develop` we run `crates/target/debug/qbz` directly, with
          # no wrapper, so replicate it here.
          shellHook = ''
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath runtimeLibs}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
          '';
        };
      });
}
