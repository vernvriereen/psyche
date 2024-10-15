import type React from "react";
import { useEffect, useRef, useState } from "react";
import { Textfit } from "react-textfit";

interface StretchTextProps extends React.HTMLAttributes<HTMLDivElement> {
  children: React.ReactNode;
  className?: string;
  style?: React.CSSProperties;
  minScale?: number;
  maxScale?: number;
  onScaleChange?: (scale: number) => void;
}

export const StretchText: React.FC<StretchTextProps> = ({ children, className, style, onScaleChange, ...props }) => {
  return (
    <Textfit mode="single" forceSingleModeWidth={false}>
      {children}
    </Textfit>
  );
};
