# `test-loader`

Test harness for a path loader.

```sh
make components/test-loader && \
  wasmtime run \
    -Wcomponent-model-map \
    -Wcomponent-model-implements \
    -Shttp \
    --dir=. \
    ./target/components/test-loader/test-loader.wasm \
    oci://ghcr.io/componentized/config/empty:0.2.1 \
    | wasm-tools component wit
```
