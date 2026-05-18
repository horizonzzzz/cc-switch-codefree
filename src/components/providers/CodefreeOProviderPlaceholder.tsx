import { useTranslation } from "react-i18next";
import { Shield } from "lucide-react";

export function CodefreeOProviderPlaceholder() {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-border p-10 text-center">
      <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-muted">
        <Shield className="h-7 w-7 text-muted-foreground" />
      </div>
      <h3 className="text-lg font-semibold">
        {t("provider.codefreeO.notSupported", { defaultValue: "Provider Management Not Supported" })}
      </h3>
      <p className="mt-2 max-w-lg text-sm text-muted-foreground">
        {t("provider.codefreeO.notSupportedDescription", { defaultValue: "This agent does not support provider management. The provider page is displayed for visual consistency with other agents only." })}
      </p>
    </div>
  );
}
