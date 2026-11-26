import { Alert, Button, Center, Group, Loader, Modal, Stack, Text } from '@mantine/core'
import { Suspense, lazy, useState } from 'react'

import { tamano } from '../formato'
import { VISIBLES_3D } from '../opciones'
import type { ArchivoFila } from '../tipos'

const Visor3D = lazy(() => import('./Visor3D'))
const PESADO = 100_000_000

/** Ventana para ver un archivo: imagen tal cual, o malla en el visor 3D (pide confirmación si pesa mucho). */
export default function VisorModal({ archivo, cerrar }: { archivo: ArchivoFila | null; cerrar: () => void }) {
  const [confirmado, setConfirmado] = useState<number | null>(null)
  const a = archivo
  const url = a ? `/api/v1/archivos/${a.id}/contenido` : ''
  const pesado = a !== null && a.bytes > PESADO && confirmado !== a.id

  return (
    <Modal opened={a !== null} onClose={cerrar} title={a?.nombre} size="xl" centered>
      {a && (
        <Stack>
          {a.tipo === 'imagen' ? (
            <img src={url} alt={a.nombre} style={{ maxWidth: '100%', maxHeight: '75vh', objectFit: 'contain' }} />
          ) : !VISIBLES_3D.includes(a.ext) ? (
            <Text size="sm">El visor no abre archivos .{a.ext}; puedes descargarlo.</Text>
          ) : pesado ? (
            <Alert color="yellow" variant="light" title={`Pesa ${tamano(a.bytes)}`}>
              <Text size="sm">Cargarlo en el navegador puede tardar y usar mucha memoria (sobre todo en el celular).</Text>
              <Button mt="xs" size="xs" variant="light" onClick={() => setConfirmado(a.id)}>
                Cargar igual
              </Button>
            </Alert>
          ) : (
            <Suspense
              fallback={
                <Center h="70vh">
                  <Loader />
                </Center>
              }
            >
              <Visor3D archivoId={a.id} ext={a.ext} />
            </Suspense>
          )}
          <Group justify="space-between">
            <Text size="xs" c="dimmed">
              {tamano(a.bytes)}
              {a.tipo !== 'imagen' && ' · arrastra para girar, rueda o pellizco para acercar'}
            </Text>
            <Button size="xs" variant="default" component="a" href={url} download={a.nombre}>
              Descargar
            </Button>
          </Group>
        </Stack>
      )}
    </Modal>
  )
}
