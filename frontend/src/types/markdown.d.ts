// Markdown files are imported as plain text (see the asset/source rule in next.config.js)
declare module '*.md' {
  const text: string;
  export default text;
}
