import { Pause, Play } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { Slider } from "@/components/ui/slider";

const formatClock = (seconds: number) => {
  const total = Math.max(0, Math.floor(seconds));
  const mins = Math.floor(total / 60);
  const secs = total % 60;
  return `${mins}:${secs.toString().padStart(2, "0")}`;
};

export interface AudioPlayerProps {
  src: string;
  duration: number;
  loopStart?: number;
  loopEnd?: number;
}

export function AudioPlayer({ src, duration, loopStart, loopEnd }: AudioPlayerProps) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const [playing, setPlaying] = useState(false);
  const [current, setCurrent] = useState(0);

  const constrained = loopStart != null && loopEnd != null;

  useEffect(() => {
    const el = audioRef.current;
    if (el) {
      el.pause();
      el.currentTime = 0;
    }
    setPlaying(false);
    setCurrent(0);
  }, [src]);

  useEffect(() => {
    if (!constrained) return;
    const el = audioRef.current;
    if (!el) return;
    if (el.currentTime < loopStart! || el.currentTime > loopEnd!) {
      el.currentTime = loopStart!;
      setCurrent(loopStart!);
    }
  }, [constrained, loopStart, loopEnd]);

  const toggle = () => {
    const el = audioRef.current;
    if (!el) return;
    if (playing) {
      el.pause();
      return;
    }
    if (constrained && (el.currentTime < loopStart! || el.currentTime >= loopEnd!)) {
      el.currentTime = loopStart!;
    }
    void el.play();
  };

  const handleTimeUpdate = () => {
    const el = audioRef.current;
    if (!el) return;
    if (constrained && el.currentTime >= loopEnd!) {
      el.currentTime = loopStart!;
    }
    setCurrent(el.currentTime);
  };

  const seek = (value: number) => {
    const el = audioRef.current;
    if (!el) return;
    const clamped = constrained
      ? Math.min(Math.max(value, loopStart!), loopEnd!)
      : value;
    el.currentTime = clamped;
    setCurrent(clamped);
  };

  return (
    <div className="player">
      <audio
        onEnded={() => setPlaying(false)}
        onPause={() => setPlaying(false)}
        onPlay={() => setPlaying(true)}
        onTimeUpdate={handleTimeUpdate}
        preload="metadata"
        ref={audioRef}
        src={src}
      />
      <button
        aria-label={playing ? "Pause" : "Play"}
        className="player-toggle"
        onClick={toggle}
        type="button"
      >
        {playing ? <Pause size={16} /> : <Play size={16} />}
      </button>
      <span className="player-time">{formatClock(current)}</span>
      <Slider
        aria-label="Seek"
        className="player-scrub"
        max={duration}
        min={0}
        onValueChange={(value) => seek(value[0])}
        step={0.1}
        value={[Math.min(current, duration)]}
      />
      <span className="player-time">{formatClock(duration)}</span>
    </div>
  );
}
