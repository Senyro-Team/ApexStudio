import { useEffect, useRef, useState } from "react";
import { File } from "lucide-react";
export default function AssetPreview({ src }: { src: string }) {
  const host = useRef<HTMLSpanElement>(null);
  const [visible, setVisible] = useState(false),
    [failed, setFailed] = useState(false);
  useEffect(() => {
    setFailed(false);
    if (!globalThis.IntersectionObserver) {
      setVisible(true);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: "200px" },
    );
    if (host.current) observer.observe(host.current);
    return () => observer.disconnect();
  }, [src]);
  return (
    <span ref={host} className="asset-preview">
      {visible && !failed ? (
        <img
          src={src}
          loading="lazy"
          alt=""
          draggable={false}
          onError={() => setFailed(true)}
        />
      ) : (
        <File size={32} />
      )}
    </span>
  );
}
