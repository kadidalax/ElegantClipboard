import { ArrowSync16Regular } from "@fluentui/react-icons";
import { Switch } from "@/components/ui/switch";

interface PluginMeta {
  id: string;
  name: string;
  description: string;
}

const PLUGINS: PluginMeta[] = [
  {
    id: "webdav",
    name: "WebDAV 同步",
    description: "通过 WebDAV 协议在多台设备间同步剪贴板数据，支持坚果云、Nextcloud 等服务",
  },
];

type PluginsTabProps = {
  enabledMap: Record<string, boolean>;
  onToggle: (id: string, value: boolean) => void;
};

export function PluginsTab({ enabledMap, onToggle }: PluginsTabProps) {
  return (
    <div className="space-y-4">
      {PLUGINS.map((plugin) => (
        <div key={plugin.id} className="rounded-lg border bg-card">
          <div className="flex items-center justify-between p-4">
            <div className="space-y-0.5 pr-4">
              <div className="flex items-center gap-2">
                <ArrowSync16Regular className="w-4 h-4 text-muted-foreground" />
                <span className="text-sm font-medium">{plugin.name}</span>
              </div>
              <p className="text-xs text-muted-foreground">{plugin.description}</p>
            </div>
            <Switch
              checked={!!enabledMap[plugin.id]}
              onCheckedChange={(v) => onToggle(plugin.id, v)}
            />
          </div>
        </div>
      ))}
    </div>
  );
}
