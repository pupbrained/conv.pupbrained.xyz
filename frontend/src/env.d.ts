import type { AttributifyAttributes } from '@unocss/preset-attributify'

declare module '*.vue';
declare module '*.svg';

declare module '@vue/runtime-dom' {
  interface HTMLAttributes extends AttributifyAttributes {}
}
