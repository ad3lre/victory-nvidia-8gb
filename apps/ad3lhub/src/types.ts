export type Project = {
  name: string;
  repo: string;
  branch: string | null;
  gpu: string[];
  fps: number | null;
  render: number | null;
  verified: boolean;
  summary: string;
  custom: boolean;
  localExe?: string | null;
  featured?: boolean;
};

export type Settings = {
  gameDir: string;
  fix8gb: boolean;
  brightShadows: boolean;
  defaultsSet: boolean;
  fullscreen: boolean;
  installDir: string;
  later: [string, string][];
  selected: string | null;
  custom: Project[];
  seriesS: boolean;
  renderScale: number;
  extraEnv: string;
  gpuFilter: number;
};

export type Tools = {
  git: string | null;
  cargo: string | null;
  msvc: boolean;
  winget: boolean;
};

export type RepoState = {
  checking: boolean;
  installed: boolean;
  built: string | null;
  remote: string | null;
};

export type JobSnapshot = {
  running: boolean;
  repo: string | null;
  stage: string;
  log: string[];
  failed: boolean;
};

export type FilesCheck = {
  gameDir: string;
  empty: boolean;
  files: [string, boolean][];
  ok: boolean;
};

export type Catalog = {
  featured: Project;
  community: Project[];
  error: string | null;
};
