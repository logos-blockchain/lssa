{
  description = "Logos Execution Zone";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-windows"
      ];

      forAll = nixpkgs.lib.genAttrs systems;

      mkPkgs =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
    in
    {
      packages = forAll (
        system:
        let
          pkgs = mkPkgs system;
          rustToolchain = pkgs.rust-bin.stable.latest.default;
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
          src = ./.;

          commonArgs = {
            inherit src;
            buildInputs = [ pkgs.openssl ];
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.clang
              pkgs.llvmPackages.libclang.lib
            ];
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          };

          walletFfiPackage = craneLib.buildPackage (
            commonArgs
            // {
              pname = "logos-execution-zone-wallet-ffi";
              version = "0.1.0";
              cargoExtraArgs = "-p wallet-ffi";
              postInstall = ''
                mkdir -p $out/include
                cp wallet-ffi/wallet_ffi.h $out/include/
              ''
              + pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
                install_name_tool -id @rpath/libwallet_ffi.dylib $out/lib/libwallet_ffi.dylib
              '';
            }
          );
        in
        {
          wallet = walletFfiPackage;
          default = walletFfiPackage;
        }
      );
      devShells = forAll (
        system:
        let
          pkgs = mkPkgs system;
          walletFfiPackage = self.packages.${system}.wallet;
          walletFfiShell = pkgs.mkShell {
            inputsFrom = [ walletFfiPackage ];
          };
        in
        {
          wallet = walletFfiShell;
          default = walletFfiShell;
        }
      );
    };
}
