import logo from "../assets/logo.png";
import { IconRefresh } from "./Icons";

type Props = {
  name: string;
  repo: string;
  onUpdate: () => void;
  onLater: () => void;
};

export function UpdateNotice({ name, repo, onUpdate, onLater }: Props) {
  return (
    <div className="notice" role="status">
      <img className="notice-mark" src={logo} alt="" />
      <div className="notice-copy">
        <strong>
          <IconRefresh size={14} /> Update ready — {name}
        </strong>
        <span>{repo} has a newer commit online</span>
      </div>
      <div className="notice-actions">
        <button type="button" className="btn secondary" onClick={onLater}>
          Later
        </button>
        <button type="button" className="btn" onClick={onUpdate}>
          Update now
        </button>
      </div>
    </div>
  );
}
