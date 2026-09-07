import { useRef, useState } from "react";
import { Pause, Play, SpeakerHigh, SpeakerX } from "@phosphor-icons/react";

function formatTime(s: number) {
  if (!Number.isFinite(s) || s < 0) return "0:00";
  const m = Math.floor(s / 60);
  const rest = Math.floor(s % 60);
  return `${m}:${rest.toString().padStart(2, "0")}`;
}

export function DemoPlayer() {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(true);
  const [progress, setProgress] = useState(0);
  const [current, setCurrent] = useState(0);
  const [duration, setDuration] = useState(0);
  const [reducedMotion] = useState(
    () =>
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  const togglePlay = () => {
    const v = videoRef.current;
    if (!v) return;
    if (v.paused) void v.play().catch(() => {});
    else v.pause();
  };

  const toggleMute = () => {
    const v = videoRef.current;
    if (!v) return;
    v.muted = !muted;
    setMuted(!muted);
  };

  const seek = (e: React.MouseEvent<HTMLDivElement>) => {
    const v = videoRef.current;
    if (!v || !Number.isFinite(v.duration) || v.duration <= 0) return;
    const r = e.currentTarget.getBoundingClientRect();
    const ratio = Math.min(Math.max((e.clientX - r.left) / r.width, 0), 1);
    v.currentTime = ratio * v.duration;
  };

  const onSeekKey = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const v = videoRef.current;
    if (!v || !Number.isFinite(v.duration) || v.duration <= 0) return;
    if (e.key === "ArrowRight") {
      v.currentTime = Math.min(v.duration, v.currentTime + 5);
      e.preventDefault();
    } else if (e.key === "ArrowLeft") {
      v.currentTime = Math.max(0, v.currentTime - 5);
      e.preventDefault();
    } else if (e.key === "Home") {
      v.currentTime = 0;
      e.preventDefault();
    } else if (e.key === "End") {
      v.currentTime = v.duration;
      e.preventDefault();
    }
  };

  return (
    <div className="group media-outline relative w-full overflow-hidden rounded-[4px] bg-black">
      <video
        ref={(el) => {
          videoRef.current = el;
          if (el) el.muted = true;
        }}
        className="block h-auto w-full"
        src="/demo.mp4"
        poster="/poster.jpg"
        aria-label="Demo video: optionMusic playing a local track"
        autoPlay={!reducedMotion}
        muted
        loop
        playsInline
        preload="auto"
        onClick={togglePlay}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onLoadedMetadata={(e) => setDuration(e.currentTarget.duration)}
        onTimeUpdate={(e) => {
          const v = e.currentTarget;
          setCurrent(v.currentTime);
          if (Number.isFinite(v.duration) && v.duration > 0) {
            setProgress(v.currentTime / v.duration);
          }
        }}
      />

      {!playing && (
        <button
          type="button"
          onClick={togglePlay}
          aria-label="Play demo"
          className="absolute inset-0 grid place-items-center"
        >
          <span className="grid h-12 w-12 place-items-center rounded-full bg-primary text-background transition-[opacity,scale] duration-200 ease-[cubic-bezier(0,0,0.2,1)] hover:opacity-85 active:scale-[0.96]">
            <Play size={18} weight="fill" className="translate-x-[2px]" />
          </span>
        </button>
      )}

      <div className="absolute inset-x-0 bottom-0 flex items-center gap-2 bg-gradient-to-t from-black/60 to-transparent px-3 pt-6 pb-2 text-white">
        <button
          type="button"
          onClick={togglePlay}
          aria-label={playing ? "Pause demo" : "Play demo"}
          className="grid h-6 w-6 flex-none place-items-center opacity-90 transition-transform duration-200 ease-[cubic-bezier(0,0,0.2,1)] hover:opacity-100 active:scale-[0.96]"
        >
          {playing ? <Pause size={13} weight="fill" /> : <Play size={13} weight="fill" className="translate-x-px" />}
        </button>
        <div
          className="flex h-6 flex-1 cursor-pointer items-center"
          onClick={seek}
          role="slider"
          tabIndex={0}
          aria-label="Seek demo"
          aria-orientation="horizontal"
          aria-valuemin={0}
          aria-valuemax={Math.round(duration)}
          aria-valuenow={Math.round(current)}
          aria-valuetext={`${formatTime(current)} of ${formatTime(duration)}`}
          onKeyDown={onSeekKey}
        >
          <div className="h-[2px] w-full overflow-hidden rounded-full bg-white/25">
            <div className="h-full rounded-full bg-white" style={{ width: `${progress * 100}%` }} />
          </div>
        </div>
        <span className="tnum flex-none whitespace-nowrap font-mono text-[10px] opacity-80">
          {formatTime(current)} / {formatTime(duration)}
        </span>
        <button
          type="button"
          onClick={toggleMute}
          aria-label={muted ? "Unmute demo" : "Mute demo"}
          className="grid h-6 w-6 flex-none place-items-center opacity-90 transition-transform duration-200 ease-[cubic-bezier(0,0,0.2,1)] hover:opacity-100 active:scale-[0.96]"
        >
          {muted ? <SpeakerX size={14} /> : <SpeakerHigh size={14} />}
        </button>
      </div>
    </div>
  );
}
