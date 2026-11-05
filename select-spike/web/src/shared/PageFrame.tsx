import type { ReactNode, Ref } from "react";
import type { OcrPage } from "../core/ocr";

/** The scrolling page with its image; overlays go in `children`. */
export function PageFrame(props: { page: OcrPage; name: string; scale: number; children: ReactNode; pageRef?: Ref<HTMLDivElement> }) {
  const { page, name, scale } = props;
  return (
    <div className="scroller">
      <div className="page" ref={props.pageRef} style={{ width: page.widthPt * scale, height: page.heightPt * scale }}>
        <img src={`./${name}.png`} alt="" draggable={false} />
        {props.children}
      </div>
    </div>
  );
}
