/**
 * Maps the game model to the widget mood enum.
 */
function moodFor(creature: Snapshot['creature']): string {
  if (!creature) return 'unknown';
  if (creature.hunger < 0.3) return 'hungry';
  if (creature.energy < 0.3) return 'sleepy';
  if (creature.boredom > 0.7) return 'lonely';
  if (creature.mood >= 0.8) return 'happy';
  return 'content';
}
