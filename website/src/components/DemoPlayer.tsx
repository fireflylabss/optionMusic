import { useEffect, useRef, useState } from "react";
import { Pause, Play, SpeakerHigh, SpeakerX } from "@phosphor-icons/react";

export function DemoPlayer() {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(true);
  const [reducedMotion] = useState(
    () =>
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  // Muta uma única vez na montagem. (Antes o ref inline re-mutava o vídeo
  // a cada render, o que desfazia o unmute no frame seguinte.)
  useEffect(() => {
    if (videoRef.current) videoRef.current.muted = true;
  }, []);

  const togglePlay = () => {
    const v = videoRef.current;
    if (!v) return;
    if (v.paused) void v.play().catch(() => {});
    else v.pause();
  };

  const toggleMute = () => {
    const v = videoRef.current;
    if (!v) return;
    const next = !muted;
    v.muted = next;
    setMuted(next);
  };

  return (
    <div className="group media-outline relative w-full overflow-hidden rounded-[4px] bg-black">
      <video
        ref={videoRef}
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
      />

      <div className="absolute inset-x-0 bottom-0 flex items-center justify-between px-3 pt-6 pb-2 text-white">
        <button
          type="button"
          onClick={togglePlay}
          aria-label={playing ? "Pause demo" : "Play demo"}
          className="grid h-6 w-6 flex-none place-items-center opacity-90 transition-transform duration-200 ease-[cubic-bezier(0,0,0.2,1)] hover:opacity-100 active:scale-[0.96]"
        >
          {playing ? <Pause size={13} weight="fill" /> : <Play size={13} weight="fill" className="translate-x-px" />}
        </button>
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
