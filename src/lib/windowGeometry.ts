import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, PhysicalPosition, PhysicalSize } from "@tauri-apps/api/window";

interface WindowGeometry { x: number; y: number; width: number; height: number; maximized: boolean }

export async function initializeWindowGeometry(): Promise<() => void> {
  const appWindow = getCurrentWindow();
  try {
    const raw = await invoke<string | null>("get_window_geometry");
    if (raw) {
      const saved = JSON.parse(raw) as WindowGeometry;
      if (saved.maximized) await appWindow.maximize();
      else if (saved.width >= 800 && saved.height >= 600) {
        await appWindow.unmaximize();
        await appWindow.setSize(new PhysicalSize(saved.width, saved.height));
        await appWindow.setPosition(new PhysicalPosition(saved.x, saved.y));
      }
    }
  } catch { /* Browser preview and corrupt settings fall back to config defaults. */ }

  let timer: ReturnType<typeof setTimeout> | null = null;
  const save = () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(async () => {
      try {
        const [position, size, maximized] = await Promise.all([
          appWindow.outerPosition(), appWindow.outerSize(), appWindow.isMaximized(),
        ]);
        const value: WindowGeometry = { x: position.x, y: position.y, width: size.width, height: size.height, maximized };
        await invoke("set_window_geometry", { value: JSON.stringify(value) });
      } catch { /* No-op in a normal browser. */ }
    }, 250);
  };
  const unlisten = await Promise.all([appWindow.onMoved(save), appWindow.onResized(save)]).catch(() => []);
  return () => { if (timer) clearTimeout(timer); for (const fn of unlisten) fn(); };
}
