// The module `materialIcons()` in vite.config.ts generates.
declare module "virtual:material-icons" {
  const tables: {
    file: string;
    folder: string;
    folderExpanded: string;
    fileNames: Record<string, string>;
    fileExtensions: Record<string, string>;
    folderNames: Record<string, string>;
    folderNamesExpanded: Record<string, string>;
    languageIds: Record<string, string>;
  };
  export default tables;
}
