export {};

declare global {
  interface Window {
    __WINZIP_E2E__?: {
      reset: () => void;
      loadScene: (id: string) => void;
      getState: () => {
        mode: string;
        lastDest: string | null;
        sheetOpen: boolean;
        encoding: string | null;
        names: string[];
      };
    };
  }
}