import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { AppShell } from "@/components/AppShell";
import * as desktop from "@/services/desktop";
import type { CameraInfo, MicrophoneInfo, RecorderSettings } from "@/types/recorder";
import type { OidcCallbackStatus, OidcClientStatus, SecureAuthStatus } from "@/types/saas";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { toast } from "sonner";

export const Route = createFileRoute("/settings")({
  head: () => ({
    meta: [
      { title: "Settings — Recorder" },
      {
        name: "description",
        content: "Configure default devices, output directory, and app preferences.",
      },
      { property: "og:title", content: "Settings — Recorder" },
      { property: "og:description", content: "Configure Recorder preferences." },
    ],
  }),
  component: SettingsPage,
});

function SettingsPage() {
  const [settings, setSettings] = useState<RecorderSettings | null>(null);
  const [mics, setMics] = useState<MicrophoneInfo[]>([]);
  const [cams, setCams] = useState<CameraInfo[]>([]);
  const [authStatus, setAuthStatus] = useState<SecureAuthStatus | null>(null);
  const [authStatusError, setAuthStatusError] = useState(false);
  const [oidcClientStatus, setOidcClientStatus] = useState<OidcClientStatus | null>(null);
  const [oidcClientError, setOidcClientError] = useState(false);
  const [oidcCallbackStatus, setOidcCallbackStatus] = useState<OidcCallbackStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [checkingSecureStore, setCheckingSecureStore] = useState(false);
  const [checkingSignInSecurity, setCheckingSignInSecurity] = useState(false);
  const [checkingProviderConfiguration, setCheckingProviderConfiguration] = useState(false);
  const [startingCloudSignIn, setStartingCloudSignIn] = useState(false);
  const [cancellingCloudSignIn, setCancellingCloudSignIn] = useState(false);
  const [clearingSession, setClearingSession] = useState(false);

  useEffect(() => {
    let cancelled = false;

    void Promise.all([
      desktop.getSettings(),
      desktop.listMicrophones(),
      desktop.listCameras(),
    ]).then(([s, m, c]) => {
      if (cancelled) return;
      setSettings(s);
      setMics(m);
      setCams(c);
    });

    void desktop.getSecureAuthStatus().then(
      (status) => {
        if (cancelled) return;
        setAuthStatus(status);
        setAuthStatusError(false);
      },
      () => {
        if (cancelled) return;
        setAuthStatusError(true);
      },
    );

    void desktop.getOidcClientStatus().then(
      (status) => {
        if (cancelled) return;
        setOidcClientStatus(status);
        setOidcClientError(false);
      },
      () => {
        if (cancelled) return;
        setOidcClientError(true);
      },
    );

    void desktop.getOidcCallbackStatus().then((status) => {
      if (!cancelled) setOidcCallbackStatus(status);
    });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!oidcCallbackStatus?.pending) return;
    let cancelled = false;
    const timer = window.setInterval(() => {
      void desktop.getOidcCallbackStatus().then(
        (status) => {
          if (!cancelled) setOidcCallbackStatus(status);
        },
        () => undefined,
      );
    }, 1000);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [oidcCallbackStatus?.pending]);

  if (!settings) {
    return (
      <AppShell>
        <div className="surface-card animate-pulse p-8">Loading…</div>
      </AppShell>
    );
  }

  const patch = (p: Partial<RecorderSettings>) => setSettings({ ...settings, ...p });

  const save = async () => {
    setSaving(true);
    try {
      const updated = await desktop.updateSettings(settings);
      setSettings(updated);
      toast.success("Settings saved");
    } finally {
      setSaving(false);
    }
  };

  const checkSecureStore = async () => {
    setCheckingSecureStore(true);
    try {
      const probe = await desktop.probeSecureAuthStore();
      const status = await desktop.getSecureAuthStatus();
      setAuthStatus(status);
      setAuthStatusError(false);
      if (!probe.supported) {
        toast.info("Native secure account storage is unavailable on this platform.");
      } else if (probe.roundTripOk) {
        toast.success("Windows secure account storage is ready");
      } else {
        toast.error("Secure account storage did not pass its readiness check");
      }
    } catch (error) {
      setAuthStatusError(true);
      toast.error(error instanceof Error ? error.message : "Secure storage check failed");
    } finally {
      setCheckingSecureStore(false);
    }
  };

  const checkSignInSecurity = async () => {
    setCheckingSignInSecurity(true);
    try {
      const probe = await desktop.probeOidcTransaction();
      const passed =
        probe.s256Ready &&
        probe.stateRoundTripOk &&
        probe.nonceRetained &&
        probe.replayRejected &&
        probe.verifierKeptNative;
      if (passed) {
        toast.success("Local PKCE and one-time sign-in state checks passed");
      } else {
        toast.error("Sign-in security checks did not pass");
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Sign-in security check failed");
    } finally {
      setCheckingSignInSecurity(false);
    }
  };

  const checkProviderConfiguration = async () => {
    setCheckingProviderConfiguration(true);
    try {
      const status = await desktop.getOidcClientStatus();
      setOidcClientStatus(status);
      setOidcClientError(false);
      if (status.configured) {
        toast.success(
          `Pinned OIDC configuration is ready (${status.callbackMode}, ${status.scopeCount} scopes)`,
        );
      } else {
        toast.info("No OIDC provider is pinned into this build yet");
      }
    } catch (error) {
      setOidcClientError(true);
      toast.error(error instanceof Error ? error.message : "Provider configuration check failed");
    } finally {
      setCheckingProviderConfiguration(false);
    }
  };

  const startCloudSignIn = async () => {
    setStartingCloudSignIn(true);
    try {
      await desktop.startOidcSignIn();
      setOidcCallbackStatus(await desktop.getOidcCallbackStatus());
      toast.info("Your browser was opened. Complete sign-in there, then return to Recorder.");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Unable to start provider sign-in");
    } finally {
      setStartingCloudSignIn(false);
    }
  };

  const cancelCloudSignIn = async () => {
    setCancellingCloudSignIn(true);
    try {
      await desktop.cancelOidcTransaction();
      setOidcCallbackStatus(await desktop.getOidcCallbackStatus());
      toast.info("Provider sign-in cancelled");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Unable to cancel provider sign-in");
    } finally {
      setCancellingCloudSignIn(false);
    }
  };

  const clearCloudSession = async () => {
    setClearingSession(true);
    try {
      await desktop.clearSecureAuthSession();
      setAuthStatus(await desktop.getSecureAuthStatus());
      setOidcCallbackStatus(await desktop.getOidcCallbackStatus());
      setAuthStatusError(false);
      toast.success("Local cloud session cleared");
    } catch (error) {
      setAuthStatusError(true);
      toast.error(error instanceof Error ? error.message : "Unable to clear cloud session");
    } finally {
      setClearingSession(false);
    }
  };

  const authStatusTitle = authStatusError
    ? "Secure storage status unavailable"
    : authStatus === null
      ? "Checking secure storage…"
      : authStatus.supported
        ? "Windows Credential Manager ready"
        : "Native secure storage unavailable";
  const authStatusDescription = authStatusError
    ? "Recorder settings remain available. Run the readiness check to retry secure storage."
    : authStatus?.signedIn
      ? "A local cloud session is stored securely."
      : "No cloud session is stored on this device.";
  const oidcStatusTitle = oidcClientError
    ? "OIDC build configuration is invalid"
    : oidcClientStatus === null
      ? "Checking provider configuration…"
      : oidcClientStatus.configured
        ? "OIDC provider contract pinned"
        : "OIDC provider not configured";
  const oidcStatusDescription = oidcClientError
    ? "Recorder will keep cloud sign-in disabled until the build configuration is corrected."
    : oidcClientStatus?.configured
      ? `HTTPS authorization endpoint, ${oidcClientStatus.callbackMode} callback, and ${oidcClientStatus.scopeCount} scopes are fixed at build time.`
      : "This build cannot prepare a cloud sign-in request.";
  const callbackStatusText = describeCallbackStatus(oidcCallbackStatus);
  const canStartCloudSignIn =
    oidcClientStatus?.configured === true &&
    oidcClientStatus.callbackMode === "loopback" &&
    authStatus?.supported === true &&
    authStatus.signedIn === false &&
    !oidcCallbackStatus?.pending;

  return (
    <AppShell>
      <h1 className="text-3xl font-semibold tracking-tight">Settings</h1>
      <p className="mt-1 text-sm text-muted-foreground">Preferences apply to all new recordings.</p>

      <div className="mt-8 space-y-4">
        <Card
          title="Default microphone"
          description="Used as the initial selection on the recorder screen."
        >
          <Select
            value={settings.defaultMicrophoneId ?? ""}
            onValueChange={(v) => patch({ defaultMicrophoneId: v || null })}
          >
            <SelectTrigger>
              <SelectValue placeholder="System default" />
            </SelectTrigger>
            <SelectContent>
              {mics.map((m) => (
                <SelectItem key={m.id} value={m.id}>
                  {m.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Card>

        <Card
          title="Default camera"
          description="Used as the initial camera when webcam is turned on."
        >
          <Select
            value={settings.defaultCameraId ?? ""}
            onValueChange={(v) => patch({ defaultCameraId: v || null })}
          >
            <SelectTrigger>
              <SelectValue placeholder="No default" />
            </SelectTrigger>
            <SelectContent>
              {cams.map((c) => (
                <SelectItem key={c.id} value={c.id}>
                  {c.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Card>

        <Card title="Frame rate" description="Target frames per second for video capture.">
          <Select value={String(settings.fps)} onValueChange={(v) => patch({ fps: Number(v) })}>
            <SelectTrigger className="w-40">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="24">24 fps</SelectItem>
              <SelectItem value="30">30 fps · recommended</SelectItem>
              <SelectItem value="60">60 fps</SelectItem>
            </SelectContent>
          </Select>
        </Card>

        <Card title="Output directory" description="Where MP4 files are saved on this device.">
          <div className="flex gap-2">
            <Input
              value={settings.outputDirectory}
              onChange={(e) => patch({ outputDirectory: e.target.value })}
            />
            <Button
              variant="outline"
              onClick={() => toast.info("Native folder picker available in desktop build.")}
            >
              Choose…
            </Button>
          </div>
        </Card>

        <Card title="Launch at startup" description="Open Recorder automatically when you sign in.">
          <Switch
            checked={settings.launchAtStartup}
            onCheckedChange={(v) => patch({ launchAtStartup: v })}
          />
        </Card>

        <Card
          title="Show camera bubble"
          description="Overlay a floating webcam bubble on the recorded video."
        >
          <Switch
            checked={settings.showCameraBubble}
            onCheckedChange={(v) => patch({ showCameraBubble: v })}
          />
        </Card>

        <Card
          title="Cloud account"
          description="Secure local account storage for the upcoming upload and sharing service."
        >
          <div className="w-[26rem] max-w-full space-y-3">
            <div className="rounded-lg border border-border bg-muted/30 px-3 py-2 text-xs">
              <div className="font-medium text-foreground">{authStatusTitle}</div>
              <div className="mt-1 text-muted-foreground">{authStatusDescription}</div>
            </div>
            <div className="rounded-lg border border-border bg-muted/30 px-3 py-2 text-xs">
              <div className="font-medium text-foreground">{oidcStatusTitle}</div>
              <div className="mt-1 text-muted-foreground">{oidcStatusDescription}</div>
            </div>
            {callbackStatusText ? (
              <div className="rounded-lg border border-border bg-muted/30 px-3 py-2 text-xs">
                <div className="font-medium text-foreground">{callbackStatusText.title}</div>
                <div className="mt-1 text-muted-foreground">{callbackStatusText.description}</div>
              </div>
            ) : null}
            <p className="text-xs text-muted-foreground">
              Provider metadata is accepted only from compile-time settings. A configured numeric
              loopback callback can open the system browser and receive one bounded response.
              Authorization-code exchange and account creation remain disabled until signed-token
              validation is implemented.
            </p>
            <div className="flex flex-wrap justify-end gap-2">
              {authStatus?.signedIn ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={clearingSession}
                  onClick={clearCloudSession}
                >
                  {clearingSession ? "Clearing…" : "Clear local session"}
                </Button>
              ) : null}
              {oidcCallbackStatus?.pending ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={cancellingCloudSignIn}
                  onClick={cancelCloudSignIn}
                >
                  {cancellingCloudSignIn ? "Cancelling…" : "Cancel provider sign-in"}
                </Button>
              ) : oidcClientStatus?.configured ? (
                <Button
                  type="button"
                  size="sm"
                  disabled={!canStartCloudSignIn || startingCloudSignIn}
                  onClick={startCloudSignIn}
                >
                  {startingCloudSignIn ? "Opening…" : "Open provider sign-in"}
                </Button>
              ) : null}
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={checkingProviderConfiguration}
                onClick={checkProviderConfiguration}
              >
                {checkingProviderConfiguration ? "Checking…" : "Check provider configuration"}
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={checkingSignInSecurity}
                onClick={checkSignInSecurity}
              >
                {checkingSignInSecurity ? "Checking…" : "Check sign-in security"}
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={checkingSecureStore}
                onClick={checkSecureStore}
              >
                {checkingSecureStore ? "Checking…" : "Check secure storage"}
              </Button>
            </div>
          </div>
        </Card>
      </div>

      <div className="mt-8 flex justify-end">
        <Button onClick={save} disabled={saving}>
          {saving ? "Saving…" : "Save changes"}
        </Button>
      </div>
    </AppShell>
  );
}

function describeCallbackStatus(
  status: OidcCallbackStatus | null,
): { title: string; description: string } | null {
  switch (status?.stage) {
    case "waiting":
      return {
        title: "Waiting for provider response",
        description: "Complete sign-in in the browser. This listener expires after ten minutes.",
      };
    case "codeReceived":
      return {
        title: "Authorization response captured",
        description:
          "The one-time code is held only in native memory. Token exchange is not enabled yet.",
      };
    case "providerError":
      return {
        title: "Provider sign-in was not completed",
        description: "The provider returned an error or the browser flow was cancelled.",
      };
    case "timedOut":
      return {
        title: "Provider response expired",
        description: "Start sign-in again to create a new one-time state and callback listener.",
      };
    case "cancelled":
      return {
        title: "Provider sign-in cancelled",
        description: "The local callback listener and pending authorization state were cleared.",
      };
    case "failed":
      return {
        title: "Provider callback failed",
        description: "The local callback boundary stopped safely. Start sign-in again to retry.",
      };
    default:
      return null;
  }
}

function Card({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div className="surface-card flex items-center justify-between gap-6 p-5">
      <div className="min-w-0 flex-1">
        <Label className="text-sm font-medium">{title}</Label>
        <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
