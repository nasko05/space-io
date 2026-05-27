/// <reference types="vite/client" />

declare module '*?url' {
  const src: string;
  export default src;
}

declare module '*?worker' {
  const Worker: { new (): Worker };
  export default Worker;
}
