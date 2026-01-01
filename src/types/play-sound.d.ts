declare module 'play-sound' {
  interface PlayOptions {
    player?: string;
  }

  interface Player {
    play(path: string, callback?: (err: any) => void): {
      kill: () => void;
    };
  }

  function playSound(options?: PlayOptions): Player;
  export = playSound;
}
