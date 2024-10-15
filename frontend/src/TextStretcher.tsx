import { useCallback, useEffect, useLayoutEffect, useRef } from "react";

const canvas = document.createElement("canvas");
const context = canvas.getContext("2d");

/**
 * Stretches a textual child to completely fill its parent.
 */
export function TextStretcher({
  className,
  children,
}: {
  className?: string;
  children: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const textRef = useRef<HTMLDivElement>(null);
  const rescaleText = useCallback(() => {
    const container = containerRef.current;
    const textElement = textRef.current;

    if (!container || !textElement || !context) return;

    const textStyle = window.getComputedStyle(textElement);
    const fontFamily = textStyle.getPropertyValue("font-family");
    const fontSize = textStyle.getPropertyValue("font-size");

    context.font = `${fontSize} ${fontFamily}`;

    const textMetrics = context.measureText(children);
    const textWidth = textMetrics.width;
    const textHeight = textMetrics.actualBoundingBoxAscent + textMetrics.actualBoundingBoxDescent;
    const textLeftOffset = textMetrics.actualBoundingBoxLeft;

    const containerStyle = window.getComputedStyle(container);

    const paddingTop = Number.parseFloat(containerStyle.getPropertyValue("padding-top"));
    const paddingRight = Number.parseFloat(containerStyle.getPropertyValue("padding-right"));
    const paddingBottom = Number.parseFloat(containerStyle.getPropertyValue("padding-bottom"));
    const paddingLeft = Number.parseFloat(containerStyle.getPropertyValue("padding-left"));

    const containerWidth = Number.parseFloat(containerStyle.getPropertyValue("width")) - (paddingLeft + paddingRight);
    const containerHeight = Number.parseFloat(containerStyle.getPropertyValue("height")) - (paddingTop + paddingBottom);

    const scaleX = containerWidth / textWidth;
    const scaleY = containerHeight / textHeight;

    textElement.style.transform = `
      translate(${textLeftOffset}px, 0)
      scale(${scaleX}, ${scaleY})
    `;
    textElement.style.transformOrigin = "left center";
  }, [children]);

  useEffect(() => {
    const resizeObserver = new ResizeObserver(() => {
      rescaleText();
    });

    if (containerRef.current) {
      resizeObserver.observe(containerRef.current);
    }

    window.addEventListener("resize", rescaleText);
    document.fonts.addEventListener("loadingdone", rescaleText);

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener("resize", rescaleText);
      document.fonts.removeEventListener("loadingdone", rescaleText);
    };
  }, [rescaleText]);

  useLayoutEffect(() => rescaleText(), [rescaleText]);

  return (
    <div ref={containerRef} className={className} style={{ display: "flex", alignItems: "center" }}>
      <div ref={textRef} className="text-nowrap">
        {children}
      </div>
    </div>
  );
}
