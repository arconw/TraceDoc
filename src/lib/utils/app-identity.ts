import type { AppInfo } from '../types/app';

export async function applyAppInfo(
  request: () => Promise<AppInfo>,
  applyName: (name: string) => void,
  applyTitle: (title: string) => void,
) {
  const info = await request();
  applyName(info.name);
  applyTitle(`${info.name} ${info.version}`);
}
