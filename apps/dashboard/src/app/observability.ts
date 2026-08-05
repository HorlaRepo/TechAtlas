export function configuredObservabilityUrl(value = import.meta.env.VITE_ADMIN_OBSERVABILITY_URL): string | undefined {
  const candidate = value?.trim();
  if (!candidate) {
    return undefined;
  }
  try {
    const url = new URL(candidate);
    if (!matchesHttp(url) || url.username || url.password) {
      return undefined;
    }
    return url.toString();
  } catch {
    return undefined;
  }
}

function matchesHttp(url: URL): boolean {
  return url.protocol === "http:" || url.protocol === "https:";
}
