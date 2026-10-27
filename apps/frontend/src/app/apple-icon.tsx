import { ImageResponse } from 'next/og';

export const size = { width: 180, height: 180 };
export const contentType = 'image/png';

/** The home-screen icon on iOS; the browser tab uses `icon.svg`. */
export default function AppleIcon() {
  return new ImageResponse(
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'linear-gradient(135deg, #0C66E4, #1D7AFC)',
      }}
    >
      <svg width="120" height="120" viewBox="0 0 64 64">
        <path
          d="M12 22l9.5 24 10.5-17 10.5 17L52 22"
          fill="none"
          stroke="#fff"
          strokeWidth="6.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </div>,
    size,
  );
}
