// The iPhone app's landscapes, drawn for a window (plan PL6): sage hills with a small sun behind
// the home header, terracotta hills along the bottom while recording. Decorative only, drawn from
// the colour tokens so both themes follow.

// Solid mixes with the page colour, so a hill in front hides what is behind it
const fill = (token: string, share: number) => ({ fill: `color-mix(in srgb, rgb(var(--${token})) ${Math.round(share * 100)}%, rgb(var(--paper)))` });
// See-through, for the hills that lie over the live transcript
const tint = (token: string, alpha: number) => ({ fill: `rgb(var(--${token}) / ${alpha})` });
const line = (token: string, alpha: number) => ({ fill: 'none', stroke: `rgb(var(--${token}) / ${alpha})`, strokeWidth: 1.4 });

/** Layered sage hills sweeping down from the top-left corner, with a half-set sun. */
export function HomeHills({ className = '', style }: { className?: string; style?: React.CSSProperties }) {
  return (
    <svg
      style={style}
      aria-hidden="true"
      focusable="false"
      viewBox="0 0 640 280"
      preserveAspectRatio="xMinYMin meet"
      className={`pointer-events-none select-none ${className}`}
    >
      <path d="M0 0 H640 C560 40 470 70 380 88 C250 114 120 150 0 250 Z" style={fill('sage', 0.16)} />
      <path d="M0 0 H520 C450 52 360 92 270 118 C170 146 80 178 0 222 Z" style={fill('sage', 0.3)} />
      <path d="M0 0 H380 C320 50 250 84 170 108 C110 126 50 150 0 178 Z" style={fill('sage', 0.46)} />
      <path d="M0 150 C70 118 150 92 240 70 C320 50 400 30 470 0" style={line('paper', 0.55)} />
      <path d="M0 118 C60 92 130 70 200 52 C260 36 310 20 350 0" style={line('paper', 0.45)} />
      <circle cx="420" cy="132" r="17" style={fill('accent', 0.9)} />
    </svg>
  );
}

/** Terracotta hills along the bottom edge, with a small sun above the right-hand hill. */
export function RecordingHills({ className = '', style }: { className?: string; style?: React.CSSProperties }) {
  return (
    <svg
      style={style}
      aria-hidden="true"
      focusable="false"
      viewBox="0 0 1200 220"
      preserveAspectRatio="xMidYMax slice"
      className={`pointer-events-none select-none ${className}`}
    >
      <circle cx="930" cy="84" r="22" style={tint('accent', 0.75)} />
      <path d="M0 120 C220 40 420 150 640 96 C820 52 1000 70 1200 40 V220 H0 Z" style={tint('accent', 0.14)} />
      <path d="M0 160 C260 100 470 190 720 140 C900 104 1060 120 1200 96 V220 H0 Z" style={tint('accent', 0.22)} />
      <path d="M0 196 C300 160 560 214 820 184 C980 166 1100 176 1200 164 V220 H0 Z" style={tint('accent', 0.32)} />
    </svg>
  );
}
