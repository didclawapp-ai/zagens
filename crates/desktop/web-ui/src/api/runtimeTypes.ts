/**
 * Wire-format types for the runtime HTTP API (D8).
 * Generated from `docs/tech/openapi/zagens-runtime-v1.openapi.json` via `npm run generate:api-types`.
 */
import type { components } from './generated/runtime-api';

type S = components['schemas'];

export type SessionMetadata = S['SessionMetadata'];
export type SessionsListResponse = S['SessionsListResponse'];
export type SessionDetailResponse = S['SessionDetailResponse'];
export type ResumeSessionResponse = S['ResumeSessionResponse'];

export type ThreadRecord = S['ThreadRecord'];
export type ThreadDetail = S['ThreadDetail'];
export type ThreadSummary = S['ThreadSummary'];
export type TurnRecord = S['TurnRecord'];
export type TurnItemRecord = S['TurnItemRecord'];

export type CreateThreadRequest = S['CreateThreadRequest'];
export type UpdateThreadRequest = S['UpdateThreadRequest'];
export type StartTurnRequest = S['StartTurnRequest'];
export type StartTurnResponse = S['StartTurnResponse'];
export type SteerTurnRequest = S['SteerTurnRequest'];
export type StreamTurnRequest = S['StreamTurnRequest'];

export type TaskRecord = S['TaskRecord'];
export type TaskSummary = S['TaskSummary'];
export type TasksResponse = S['TasksResponse'];
export type TaskCounts = S['TaskCounts'];

export type RoutingRulesDoc = S['RoutingRulesDoc'];
export type RoutingRule = S['RoutingRule'];
export type UsageAggregation = S['UsageAggregation'];

export type RuntimeTurnStatus = S['RuntimeTurnStatus'];
export type CoherenceState = S['CoherenceState'];
