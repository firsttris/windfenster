import type { CSSProperties } from 'react';

// SVG presentation attributes as CSS, which React and TanStack Redact both write as `style`.
// Redact 0.3.1 hyphenates only clip*, fill* and stroke* attributes (attributeName() in
// src/core/attributes.ts); `textAnchor` and `vectorEffect` reach the DOM in camelCase and do nothing.

/** Text centred on its x. */
export const TEXT_CENTRED: CSSProperties = { textAnchor: 'middle' };

/** A stroke as wide on screen however the viewBox is stretched. */
export const NON_SCALING_STROKE: CSSProperties = { vectorEffect: 'non-scaling-stroke' };
