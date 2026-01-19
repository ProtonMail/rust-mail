## 🚴 Usage

Make sure that the wasm-bindgen feature is enabled.

### 🛠️ Build with `wasm-pack build`

```
wasm-pack build
```

### 🔬 Test in Headless Browsers with `wasm-pack test`

```
wasm-pack test --headless --firefox
```

### 🐳 Test in Headless Browsers with `wasm-pack test`, in a container

Check the latest version of the builder in the gitlab pipelines

```
docker run --rm -v $(pwd):/code -w /code/packages/search gitlab.protontech.ch:4567/backend-team/foundation-team/search/builder:v4 wasm-pack test --headless --firefox
```

## 🔋 Batteries Included

* [`wasm-bindgen`](https://github.com/rustwasm/wasm-bindgen) for communicating
  between WebAssembly and JavaScript.
* [`console_error_panic_hook`](https://github.com/rustwasm/console_error_panic_hook)
  for logging panic messages to the developer console.

## Release process

