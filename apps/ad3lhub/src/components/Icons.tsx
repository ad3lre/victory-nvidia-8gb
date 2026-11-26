import type { LucideProps } from "lucide-react";
import {
  AlertTriangle,
  Check,
  Download,
  Flame,
  Folder,
  Library,
  Play,
  RefreshCw,
  Settings,
  Terminal,
  Users,
} from "lucide-react";

type Props = Pick<LucideProps, "size" | "className" | "strokeWidth">;

const defaults: Props = { size: 16, strokeWidth: 1.75 };

export function IconPlay(props: Props) {
  return <Play {...defaults} {...props} />;
}

export function IconDownload(props: Props) {
  return <Download {...defaults} {...props} />;
}

export function IconFlame(props: Props) {
  return <Flame {...defaults} {...props} />;
}

export function IconFolder(props: Props) {
  return <Folder {...defaults} {...props} />;
}

export function IconGear(props: Props) {
  return <Settings {...defaults} {...props} />;
}

export function IconUsers(props: Props) {
  return <Users {...defaults} {...props} />;
}

export function IconTerminal(props: Props) {
  return <Terminal {...defaults} {...props} />;
}

export function IconCheck(props: Props) {
  return <Check {...defaults} {...props} />;
}

export function IconAlert(props: Props) {
  return <AlertTriangle {...defaults} {...props} />;
}

export function IconLibrary(props: Props) {
  return <Library {...defaults} {...props} />;
}

export function IconRefresh(props: Props) {
  return <RefreshCw {...defaults} {...props} />;
}
