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
  lleno = false,
}: {
  archivoId: number
  clave: string | null | undefined
  tamano?: 128 | 512
  lado: number
  tipo?: string
  ext?: string
  /** Ocupa todo el ancho de su caja (cuadrada); `lado` solo decide la resolución. */
  lleno?: boolean
}) {
  const caja = lleno ? { width: '100%', aspectRatio: '1 / 1' } : { width: lado, height: lado }
  if (!clave) {
    return (
      <Center
        style={{ ...caja, borderRadius: 6, background: `var(--mantine-color-${COLOR_TIPO[tipo ?? ''] ?? 'gray'}-light)` }}
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
      width={lleno ? undefined : lado}
      height={lleno ? undefined : lado}
      style={{ ...caja, objectFit: 'contain', borderRadius: 6, display: 'block' }}
    />
  )
}
