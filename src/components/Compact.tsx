import type { Media } from "../api";
import { IMusic } from "./Icons";

function Cover({ url, size, radius }: { url: string | null; size: number; radius: number }) {
  return url ? (
    <img className="cover" src={url} alt="" style={{ width: size, height: size, borderRadius: radius }} draggable={false} />
  ) : (
    <div className="cover cover-empty" style={{ width: size, height: size, borderRadius: radius }}>
      <IMusic size={size * 0.55} />
    </div>
  );
}

/** Music playing, island collapsed: artwork on the left, bars are drawn on the canvas. */
export function Compact({ cover }: { cover: string | null }) {
  return (
    <div className="compact">
      <Cover url={cover} size={24} radius={7} />
    </div>
  );
}

/** Track changed: show title and artist for a moment. */
export function Peek({ media, cover }: { media: Media; cover: string | null }) {
  return (
    <div className="peek">
      <Cover url={cover} size={44} radius={11} />
      <div className="peek-text">
        <div className="peek-title">{media.title}</div>
        <div className="peek-artist">{media.artist || media.app}</div>
      </div>
    </div>
  );
}

export { Cover };
