export const profileSchema = z.object({
  id: z.string(),
  displayName: z.string(),
  email: z.string().email().nullable(),
  guest: z.boolean(),
});
export type Profile = z.infer<typeof profileSchema>;
