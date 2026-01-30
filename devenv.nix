{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

let
  # Sometimes we have to escape paths provided by nix in order to cross compile properly the rust code in iOS
  filterPkgIn =
    pkg: variable:
    "${variable}=\"$(echo \"\$${variable}\" | tr \":\" \"\\n\" | grep -v \"${pkg}\" | paste -sd \":\")\"";

  filterPkg =
    pkg:
    let
      vars = [
        "PATH"
        "NIX_CFLAGS_COMPILE"
        "NIX_LDFLAGS"
        "XDG_DATA_DIRS"
      ];

    in
    lib.strings.concatMapStringsSep " " (var: filterPkgIn pkg var) vars;

in
{
  enterShell = pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
    # NOTE: on macOS, Go and other derivations bring in an 'xcodebuild' dependency
    # that will mess with the native Xcode.app, preventing developers from running
    # the iOS app on their machines
    #
    # Here we're filtering the /bin path to the 'xcodebuild' dependency brought in,
    # so that 'xcodebuild' resolves to the version installed outside the devshell.
    export ${filterPkgIn "xcbuild" "PATH"};
    export ${filterPkgIn "clang" "PATH"};
    export ${filterPkgIn "cctools-binutils" "PATH"};
    unset DEVELOPER_DIR;
    unset SDKROOT;
    unset LD;

    echo "Helper scripts you can run to make your development richer:"
    echo
    ${pkgs.gnused}/bin/sed -e 's| |••|g' -e 's|=| |' <<EOF | ${pkgs.util-linuxMinimal}/bin/column -t | ${pkgs.gnused}/bin/sed -e 's|^|  |' -e 's|••| |g'
    ${lib.generators.toKeyValue { } (lib.mapAttrs (name: value: value.description) config.scripts)}
    EOF
  '';

  overlays = [ inputs.rust-overlay.overlays.default ];

  packages =
    let
      rustVersion = (
        let
          file = builtins.readFile ./rust-toolchain.toml;
          toml = builtins.fromTOML file;
        in toml.toolchain.channel
      );

      rustTargets =
        [
          "wasm32-unknown-unknown"
        ]
        ++ lib.optionals pkgs.stdenv.isDarwin [
          "aarch64-apple-ios"
          "aarch64-apple-ios-sim"
        ];

      rustToolchain = pkgs.rust-bin.stable.${rustVersion}.default.override {
        targets = rustTargets;
      };
    in
    with pkgs;
    [
      rustToolchain
      bashInteractive
      git-cliff
      php # For iCal
      php.unwrapped.dev # For iCal
      sql-formatter
      cargo-nextest
      cargo-insta
    ]
    ++ lib.optionals pkgs.stdenv.isDarwin (
      with pkgs;
      [
        xcodes # Selector of the xcode version
        findutils
        libiconv
      ]
    );

  languages = {
   # rust.enable = true; # This is disabled because we are using rust-overlay directly

    go = {
      enable = true; # For PGP
    };

    python = {
      enable = true; # For changelog
      uv = {
        enable = true;
      };
    };

    php = {
      enable = true; # For iCal
    };
  };

  scripts = {
    proton-install-xcode = {
      description = "Installs Xcode.";
      binary = "bash";

      exec = ''
        xcodes install 16.3
      '';
    };

    proton-logs-ios = {
      description = "Shows the rust logs of the iOS app";
      binary = "bash";

      exec = ''
        pushd "$DEVENV_ROOT"

        xcrun simctl spawn "$DEVICE_ID" log stream \
              --predicate 'subsystem == "ch.protonmail.protonmail" AND category == "[Proton] Rust"' \
              --style syslog

        popd
      '';
    };

    proton-run-ios = {
      description = ''Builds the iOS project (but not the uniffi framework) and runs it on the simulator'';
      binary = "bash";

      exec = ''
        pushd "$DEVENV_ROOT"

        ./mail/mail-uniffi/ios/run-local.sh

        popd
      '';
    };
    proton-build-ios = {
      description = "Builds iOS uniffi framework and injects it to the iOS project";
      binary = "bash";

      exec = ''
        pushd "$DEVENV_ROOT"

        ${filterPkg "libiconv"} ./mail/mail-uniffi/ios/build-local.sh

        popd
      '';
    };
  };
}
