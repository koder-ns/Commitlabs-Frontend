import { randomUUID } from 'crypto';

export type AuditEventType =
  | 'DISPUTE_OPENED'
  | 'DISPUTE_RESOLVED'
  | 'DISPUTE_RESOLVED_FAILED'
  | 'DISPUTE_OPEN_FAILED';

export interface AuditLogEntry {
  id: string;
  eventType: AuditEventType;
  timestamp: string;
  actorAddress: string;
  commitmentId: string;
  details: Record<string, unknown>;
}

const auditLogStore: AuditLogEntry[] = [];

export function recordAuditEvent(
  entry: Omit<AuditLogEntry, 'id' | 'timestamp'>,
): AuditLogEntry {
  const logEntry: AuditLogEntry = {
    id: randomUUID(),
    timestamp: new Date().toISOString(),
    ...entry,
  };

  auditLogStore.push(logEntry);
  console.log(JSON.stringify({ event: 'AuditLog', ...logEntry }));

  return logEntry;
}

export function getAuditLog(commitmentId: string): AuditLogEntry[] {
  return auditLogStore.filter((entry) => entry.commitmentId === commitmentId);
}

export function clearAuditLog(): void {
  auditLogStore.length = 0;
}

export type AuditEventCategory =
  | 'commitment'
  | 'attestation'
  | 'marketplace'
  | 'auth'
  | 'admin';

export type AuditEventSeverity = 'info' | 'warn' | 'error';

export interface AuditEvent {
  id: string;
  timestamp: string;
  category: AuditEventCategory;
  action: string;
  severity: AuditEventSeverity;
  actor?: string;
  resourceId?: string;
  metadata?: Record<string, unknown>;
  ip?: string;
}

export type RedactedAuditEvent = Omit<AuditEvent, 'actor' | 'ip'> & {
  actor: string;
  ip: string;
};
export interface AuditEventFilters {
  actor?: string;
  type?: string;
  startTime?: string;
  endTime?: string;
}

const REDACTED = '[REDACTED]';
const SENSITIVE_METADATA_KEYS = new Set([
  'ownerAddress',
  'verifiedBy',
  'callerAddress',
  'sellerAddress',
  'privateKey',
  'secret',
  'token',
  'password',
]);

export function redactAuditEvent(event: AuditEvent): RedactedAuditEvent {
  const redactedMetadata = event.metadata
    ? Object.fromEntries(
        Object.entries(event.metadata).map(([key, value]) =>
          SENSITIVE_METADATA_KEYS.has(key) ? [key, REDACTED] : [key, value],
        ),
      )
    : undefined;

  return {
    ...event,
    actor: REDACTED,
    ip: REDACTED,
    ...(redactedMetadata !== undefined ? { metadata: redactedMetadata } : {}),
  };
}

export interface AuditStore {
  append(event: AuditEvent): void | Promise<void>;
  recent(limit: number): AuditEvent[] | Promise<AuditEvent[]>;
  size(): number | Promise<number>;
}

const MAX_BUFFER_SIZE = 500;

class InMemoryAuditStore implements AuditStore {
  private readonly buffer: AuditEvent[] = [];

  append(event: AuditEvent): void {
    this.buffer.push(event);
    if (this.buffer.length > MAX_BUFFER_SIZE) {
      this.buffer.shift();
    }
  }

  recent(limit: number): AuditEvent[] {
    return this.buffer.slice(-limit).reverse();
  }

  size(): number {
    return this.buffer.length;
  }

  clear(): void {
    this.buffer.length = 0;
  }
}

const inMemoryStore = new InMemoryAuditStore();
let activeStore: AuditStore = inMemoryStore;

export function setAuditStore(store: AuditStore): void {
  activeStore = store;
}

export function resetAuditStoreForTests(): void {
  inMemoryStore.clear();
  activeStore = inMemoryStore;
}

const TRUE_VALUES = new Set(['1', 'true', 'yes', 'on']);

export function isAuditLogEnabled(): boolean {
  const raw = process.env.COMMITLABS_FEATURE_AUDIT_LOG;
  if (raw === undefined) return false;
  return TRUE_VALUES.has(raw.trim().toLowerCase());
}

function generateId(): string {
  return randomUUID();
}

export async function appendAuditEvent(
  event: Omit<AuditEvent, 'id' | 'timestamp'>,
): Promise<void> {
  if (!isAuditLogEnabled()) return;

  await activeStore.append({
    id: generateId(),
    timestamp: new Date().toISOString(),
    ...event,
  });
}

export async function getRecentAuditEvents(
  limit: number,
): Promise<RedactedAuditEvent[]> {
  if (!isAuditLogEnabled()) return [];
  const events = await activeStore.recent(limit);
  return events.map(redactAuditEvent);
}

export async function getAuditEventCount(): Promise<number> {
  if (!isAuditLogEnabled()) return 0;

  const hasFilters =
    filters !== undefined &&
    (filters.actor !== undefined ||
      filters.type !== undefined ||
      filters.startTime !== undefined ||
      filters.endTime !== undefined);

  if (hasFilters) {
    return filterAuditEvents(getAllStoredAuditEvents(), filters).length;
  }

  return activeStore.size();
}
