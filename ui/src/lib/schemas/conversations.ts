// Conversations, projects and search (docs/ui-contract.md, "Types" and v1 argument shapes).
import * as z from "zod/mini";

export const ConvSummarySchema = z.object({
  id: z.string(),
  title: z.string(),
  project_id: z.nullable(z.string()),
  pinned: z.boolean(),
  archived: z.boolean(),
  created_at: z.number(),
  updated_at: z.number(),
});

/** `items` is opaque to the backend; the UI owns its shape (see lib/chat.ts). */
export const ConversationSchema = z.extend(ConvSummarySchema, { items: z.array(z.unknown()) });

export const ProjectSchema = z.object({
  id: z.string(),
  name: z.string(),
  color: z.string(),
  instructions: z.string(),
  created_at: z.number(),
  updated_at: z.number(),
});

export const SearchResultSchema = z.object({
  conversations: z.array(z.extend(ConvSummarySchema, { snippet: z.string() })),
  projects: z.array(ProjectSchema),
});

export type ConvSummary = z.infer<typeof ConvSummarySchema>;
export type Conversation = z.infer<typeof ConversationSchema>;
export type Project = z.infer<typeof ProjectSchema>;
export type SearchResult = z.infer<typeof SearchResultSchema>;
