// /installer: shows the setup or uninstall screen (development and tests; the release exe
// routes by installer_info before the app renders, see main.tsx).
import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/backend";
import Installer from "@/pages/Installer";
import { Skeleton } from "@/components/ui/skeleton";

export default function InstallerRoute() {
  const info = useQuery({ queryKey: ["installer-info"], queryFn: () => api.installerInfo(), staleTime: Infinity });
  if (!info.data) return <Skeleton className="m-10 h-40" />;
  const mode = info.data.mode === "app" ? "install" : info.data.mode;
  return <Installer info={{ ...info.data, mode }} />;
}
