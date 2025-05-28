{ pkgs, lib, config, ... }:

# Before you start
# Make sure, that you have created `devenv.local.nix` file with following content
# ```
# { pkgs, ... }:
# {
# env.IOS_REPO_ROOT="<path to your ET apple inbox repository>";
# }
# ```

let
 # Sometimes we have to escape paths provided by nix in order to cross compile properly the rust code in iOS
 filterPkgIn = pkg: variable: "${variable}=\"$(echo \"\$${variable}\" | tr \":\" \"\\n\" | grep -v \"${pkg}\" | paste -sd \":\")\"";
 filterPkg = pkg: 
  let vars = ["PATH" "NIX_CFLAGS_COMPILE" "NIX_LDFLAGS" "XDG_DATA_DIRS"];
  in lib.strings.concatMapStringsSep " " (var: filterPkgIn pkg var) vars;
in
{
  packages = [
    pkgs.bashInteractive
    pkgs.php # For iCal
    pkgs.php.unwrapped.dev # For iCal
    pkgs.sql-formatter
    pkgs.git-cliff
  ] ++ lib.optionals pkgs.stdenv.isDarwin (
    with pkgs; [
      libiconv
      findutils
      darwin.xcode_16_3
      # pkgsCross.x86_64-darwin.apple-sdk_15
    ]
  );

  languages.rust = {
    enable = true;
    channel = "stable";
    targets = [
      "wasm32-unknown-unknown"
    ] ++ lib.optionals pkgs.stdenv.isDarwin [
      # iOS cross compilation
      "aarch64-apple-ios"
      "aarch64-apple-ios-sim"
      # "x86_64-apple-ios"
    ];
  };
  languages.go.enable = true; # For PGP 
  languages.python.enable = true; # For changelog
  languages.python.uv.enable = true;
  languages.php.enable = true; # For iCal
  
  scripts.xcode = if pkgs.stdenv.isDarwin then {
    description = "Opens XCode";
    binary = "bash";
    exec = ''
      open -n "${pkgs.darwin.xcode_16_3}"
    '';
  } else null;

  scripts.proton-build-ios = {
    description = "Builds iOS uniffi framework and injects it to the iOS project";
    binary = "bash";
    exec = ''
      pushd "$DEVENV_ROOT"
  
      ${filterPkg "libiconv"} ./mail/mail-uniffi/ios/build-local.sh

      popd
    '';
  };

  enterShell = pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
    # NOTE: on macOS, Go and other derivations bring in an 'xcodebuild' dependency
    # that will mess with the native Xcode.app, preventing developers from running
    # the iOS app on their machines 
    #
    # Here we're filtering the /bin path to the 'xcodebuild' dependency brought in,
    # so that 'xcodebuild' resolves to the version installed outside the devshell.
    export ${filterPkgIn "xcbuild" "PATH"};
    export ${filterPkgIn "clang" "PATH"};
    unset DEVELOPER_DIR;
    unset SDKROOT;

    echo "Helper scripts you can run to make your development richer:"
    echo 
    ${pkgs.gnused}/bin/sed -e 's| |••|g' -e 's|=| |' <<EOF | ${pkgs.util-linuxMinimal}/bin/column -t | ${pkgs.gnused}/bin/sed -e 's|^|  |' -e 's|••| |g'
    ${lib.generators.toKeyValue {} (lib.mapAttrs (name: value: value.description) config.scripts)}
    EOF
   
  '';
}
