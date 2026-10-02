{
  description = "lana – Hack am Rhein challenge #3 (Tauri + Rust + SQLite)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
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
      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          lib = pkgs.lib;

          # Linux-only system libraries needed to build/run WebKitGTK (Tauri).
          # On macOS Tauri uses the system WebKit, so nothing extra is needed.
          linuxGuiDeps = lib.optionals pkgs.stdenv.hostPlatform.isLinux (with pkgs; [
            glib
            gtk3
            webkitgtk_4_1
            libsoup_3
            cairo
            pango
            gdk-pixbuf
            librsvg
            gobject-introspection
            glib-networking # TLS support inside WebKitGTK
            dbus
            openssl
            curl
            wget
          ]);

          commonDeps = with pkgs; [
            # Rust toolchain (pinned by flake.lock via nixpkgs)
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer

            # Tauri CLI (tauri dev / tauri build)
            cargo-tauri

            # Frontend tooling – extend once the framework is chosen
            nodejs

            # DB
            sqlite # CLI for inspecting the app database

            # Misc
            just
            pkg-config
          ];
        in
        {
          default = pkgs.mkShell {
            buildInputs = linuxGuiDeps;
            nativeBuildInputs = commonDeps;

            env = lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
              # Runtime lookup for shared libs when running the debug binary
              LD_LIBRARY_PATH = lib.makeLibraryPath (with pkgs; [
                webkitgtk_4_1
                gtk3
                glib
                gobject-introspection
                gdk-pixbuf
                pango
                cairo
                librsvg
                libsoup_3
                dbus
                openssl
                curl
              ]);
              # SVG loading support for gdk-pixbuf
              GDK_PIXBUF_MODULE_FILE = "${pkgs.librsvg}/lib/gdk-pixbuf/loaders.cache";
              # glib networking modules (TLS) for WebKitGTK
              GIO_MODULE_DIR = "${pkgs.glib-networking}/lib/gio/modules/";
            };

            shellHook = ''
              echo "lana dev shell – rust $(rustc --version | cut -d' ' -f2), node $(node --version), sqlite $(sqlite3 --version | cut -d' ' -f1)"
              echo "quick start: just dev"
            '';
          };
        });
    };
}
