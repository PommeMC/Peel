{
  pkgs ? import <nixpkgs> {},
}:
let
  wasm-bindgen-cli_0_2_128 = pkgs.buildWasmBindgenCli rec {
    src = pkgs.fetchCrate {
      pname = "wasm-bindgen-cli";
      version = "0.2.128";
      hash = "sha256-a7lcXJnnZkYReja+iUO7NqqrWyv3toxnUgQb8s4IS5s=";
    };

    cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
      inherit src;
      inherit (src) pname version;
      hash = "sha256-R1Tas33Ursy8kqsxguAkG0ZhNed2n5uFTAhw1l2qlLY=";
    };
  };
in
pkgs.mkShell {
  packages = with pkgs; [
    cargo
    just
    lld
    nodejs
    pnpm
    wasm-bindgen-cli_0_2_128
  ];
}
