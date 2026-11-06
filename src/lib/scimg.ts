/** Sizes the `scimg://` protocol accepts (SoundCloud CDN size suffixes). */
export type ImageSize = 't67x67' | 'large' | 't200x200' | 't300x300' | 't500x500' | 'original';

function base64Url(text: string): string {
  let binary = '';
  for (const byte of new TextEncoder().encode(text)) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/** An image URL for `<img>`: always through the Rust cache, never sndcdn directly. */
export function scimg(url: string | null | undefined, size: ImageSize): string | undefined {
  if (!url) return undefined;
  return `http://scimg.localhost/${size}/${base64Url(url)}`;
}
