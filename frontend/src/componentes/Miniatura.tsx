import { Center, Text } from '@mantine/core'

import { COLOR_TIPO } from '../formato'

/** Miniatura de un archivo (la clave va en la URL: el navegador la guarda). Sin miniatura, muestra el tipo. */
export default function Miniatura({
  archivoId,
  clave,
  tamano = 128,
  lado,
  tipo,
  ext,
}: {
  archivoId: number
  clave: string | null | undefined
  tamano?: 128 | 512
  lado: number
  tipo?: string
  ext?: string
}) {
  if (!clave) {
    return (
      <Center
        w={lado}
        h={lado}
        style={{ borderRadius: 6, background: `var(--mantine-color-${COLOR_TIPO[tipo ?? ''] ?? 'gray'}-light)` }}
      >
        <Text size="xs" c="dimmed" tt="uppercase">
          {ext || tipo || '—'}
        </Text>
      </Center>
    )
  }
  return (
    <img
      src={`/api/v1/archivos/${archivoId}/miniatura?t=${tamano}&v=${clave}`}
      alt=""
      loading="lazy"
      width={lado}
      height={lado}
      style={{ objectFit: 'contain', borderRadius: 6, display: 'block' }}
    />
  )
}
