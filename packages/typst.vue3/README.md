# Typst.vue3 [WIP]

This is a basic vue component for rendering typst documents.

## Installation

```bash
yarn add @myriaddreamin/typst.ts
yarn add @myriaddreamin/typst.vue3
```

## Usage

use the component:

```vue
<template>
  <Typst v-bind:content="sourceCode" />
</template>
```

## Loading Wasm from Other Places

Set path to wasm files:

```ts
import { $typst } from '@myriaddreamin/typst.ts';

$typst.setCompilerInitOptions({
  beforeBuild: [],
  getModule: () =>
    'https://cdn.jsdelivr.net/npm/@myriaddreamin/typst-ts-web-compiler/pkg/typst_ts_web_compiler_bg.wasm',
});

$typst.setRendererInitOptions({
  beforeBuild: [],
  getModule: () =>
    'https://cdn.jsdelivr.net/npm/@myriaddreamin/typst-ts-renderer/pkg/typst_ts_renderer_bg.wasm',
});
```

## Documentation

See [Vue3 Library Docs](https://myriad-dreamin.github.io/typst.ts/cookery/guide/renderer/vue3.html).

## Development

Run the example project:

```bash
# At the root of the project, run the typst-ts-dev-server, serving local wasm files.
yarn dev
# In another terminal, run the example project.
cd packages/typst.vue3 && yarn dev
```
