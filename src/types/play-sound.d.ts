declare module "play-sound" {
  interface PlayOptions {
    player?: string;
  }

  interface Player {
    play(
      path: string,
      callback?: (err: Error | null) => void,
    ): {
      kill: () => void;
    };
  }

  function playSound(options?: PlayOptions): Player;
  export = playSound;
}
