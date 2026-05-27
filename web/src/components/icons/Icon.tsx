import { CSSProperties, SVGProps } from 'react';

// Ported from /tmp/ui-bundle/hearth-bundle/shared-ui.jsx:4-165.
// Stroke-based, 1.5-1.6px, currentColor — sits well next to serif type.

export interface IconProps extends Omit<SVGProps<SVGSVGElement>, 'size'> {
  size?: number;
  style?: CSSProperties;
}

const wrap = (
  body: React.ReactNode,
  defaultStrokeWidth: number | undefined,
  { size = 16, ...rest }: IconProps,
  fillMode: 'stroke' | 'fill' = 'stroke',
) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill={fillMode === 'fill' ? 'currentColor' : 'none'}
    stroke={fillMode === 'stroke' ? 'currentColor' : 'none'}
    strokeWidth={defaultStrokeWidth}
    strokeLinecap="round"
    strokeLinejoin="round"
    {...rest}
  >
    {body}
  </svg>
);

export const Search = (p: IconProps) =>
  wrap(
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="m21 21-4.3-4.3" />
    </>,
    1.6,
    p,
  );

export const Plus = (p: IconProps) =>
  wrap(<path d="M12 5v14M5 12h14" />, 1.6, p);

export const Folder = (p: IconProps) =>
  wrap(<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z" />, 1.6, p);

export const FolderOpen = (p: IconProps) =>
  wrap(
    <>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v1H3V7Z" />
      <path d="M3 9h18l-2 8a2 2 0 0 1-2 1.5H5A2 2 0 0 1 3 17V9Z" />
    </>,
    1.6,
    p,
  );

export const FileText = (p: IconProps) =>
  wrap(
    <>
      <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9l-6-6Z" />
      <path d="M14 3v6h6M8 13h8M8 17h5" />
    </>,
    1.6,
    p,
  );

export const FilePdf = ({ size = 16, ...rest }: IconProps) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.6"
    strokeLinecap="round"
    strokeLinejoin="round"
    {...rest}
  >
    <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9l-6-6Z" />
    <path d="M14 3v6h6" />
    <text x="7.5" y="18" fontSize="5.5" fontWeight={700} stroke="none" fill="currentColor" fontFamily="ui-sans-serif, system-ui">
      PDF
    </text>
  </svg>
);

export const FileDocx = ({ size = 16, ...rest }: IconProps) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.6"
    strokeLinecap="round"
    strokeLinejoin="round"
    {...rest}
  >
    <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9l-6-6Z" />
    <path d="M14 3v6h6" />
    <text x="7" y="18" fontSize="5" fontWeight={700} stroke="none" fill="currentColor" fontFamily="ui-sans-serif, system-ui">
      DOC
    </text>
  </svg>
);

export const Image = (p: IconProps) =>
  wrap(
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <circle cx="9" cy="10" r="2" />
      <path d="m21 17-5-5-9 9" />
    </>,
    1.6,
    p,
  );

export const Video = (p: IconProps) =>
  wrap(
    <>
      <rect x="3" y="5" width="14" height="14" rx="2" />
      <path d="m17 9 4-2v10l-4-2V9Z" />
    </>,
    1.6,
    p,
  );

export const Play = (p: IconProps) => wrap(<path d="M8 5v14l11-7L8 5Z" />, undefined, p, 'fill');

export const Tag = (p: IconProps) =>
  wrap(
    <>
      <path d="M20 12.5 12.5 20a2 2 0 0 1-2.8 0L3 13.3V4h9.3l7.7 7.7a2 2 0 0 1 0 .8Z" />
      <circle cx="8" cy="8" r="1.4" fill="currentColor" />
    </>,
    1.6,
    p,
  );

export const Chevron = ({ size = 14, ...rest }: IconProps) =>
  wrap(<path d="m9 6 6 6-6 6" />, 2, { size, ...rest });

export const ChevronDown = ({ size = 14, ...rest }: IconProps) =>
  wrap(<path d="m6 9 6 6 6-6" />, 2, { size, ...rest });

export const Sun = (p: IconProps) =>
  wrap(
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </>,
    1.6,
    p,
  );

export const Moon = (p: IconProps) =>
  wrap(<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z" />, 1.6, p);

export const Upload = (p: IconProps) =>
  wrap(<path d="M12 16V4M7 9l5-5 5 5M4 20h16" />, 1.6, p);

export const Settings = (p: IconProps) =>
  wrap(
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3h0a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8h0a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" />
    </>,
    1.6,
    p,
  );

export const Link = (p: IconProps) =>
  wrap(
    <>
      <path d="M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1.5 1.5" />
      <path d="M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1.5-1.5" />
    </>,
    1.6,
    p,
  );

export const Clock = (p: IconProps) =>
  wrap(
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </>,
    1.6,
    p,
  );

export const Commit = (p: IconProps) =>
  wrap(
    <>
      <circle cx="12" cy="12" r="3.5" />
      <path d="M3 12h5.5M15.5 12H21" />
    </>,
    1.6,
    p,
  );

export const Branch = (p: IconProps) =>
  wrap(
    <>
      <circle cx="6" cy="5" r="2" />
      <circle cx="6" cy="19" r="2" />
      <circle cx="18" cy="7" r="2" />
      <path d="M6 7v10M18 9c0 4-6 4-6 8" />
    </>,
    1.6,
    p,
  );

export const Sparkle = (p: IconProps) =>
  wrap(
    <path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.5 5.5l2 2M16.5 16.5l2 2M5.5 18.5l2-2M16.5 7.5l2-2" />,
    1.6,
    p,
  );

export const Eye = (p: IconProps) =>
  wrap(
    <>
      <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z" />
      <circle cx="12" cy="12" r="3" />
    </>,
    1.6,
    p,
  );

export const Pencil = (p: IconProps) =>
  wrap(
    <>
      <path d="M3 21h4l11-11-4-4L3 17v4Z" />
      <path d="m14 6 4 4" />
    </>,
    1.6,
    p,
  );

export const Close = (p: IconProps) => wrap(<path d="M6 6l12 12M18 6 6 18" />, 1.8, p);

export const MoreHorizontal = (p: IconProps) =>
  wrap(
    <>
      <circle cx="5" cy="12" r="1.4" fill="currentColor" />
      <circle cx="12" cy="12" r="1.4" fill="currentColor" />
      <circle cx="19" cy="12" r="1.4" fill="currentColor" />
    </>,
    0,
    p,
  );

export const Split = (p: IconProps) =>
  wrap(
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M12 4v16" />
    </>,
    1.6,
    p,
  );

export const Download = (p: IconProps) => wrap(<path d="M12 4v12M7 11l5 5 5-5M4 20h16" />, 1.6, p);

export const Pin = (p: IconProps) => wrap(<path d="M12 17v5M8 3h8l-1 6 3 3H6l3-3-1-6Z" />, 1.6, p);

export const HardDrive = (p: IconProps) =>
  wrap(
    <>
      <path d="M3 13h18M5 13 7 5h10l2 8v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6Z" />
      <circle cx="8" cy="17" r="1" fill="currentColor" />
    </>,
    1.6,
    p,
  );
