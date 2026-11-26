import { Alert, Button, Card, Group, SimpleGrid, Text } from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { Link } from 'react-router'

import { api } from '../api'
import { n } from '../formato'
import type { ListaPlacas } from '../tipos'
import TarjetaPlaca from './TarjetaPlaca'

const PRIMERAS = 12

/** Las placas de un modelo (las sugeridas y las confirmadas), con su laminado y para qué impresora. */
export default function PlacasDelModelo({ modeloId }: { modeloId: number }) {
  const qc = useQueryClient()
  const [todas, setTodas] = useState(false)
  const q = useQuery({
    queryKey: ['placas-modelo', modeloId],
    queryFn: () => api<ListaPlacas>(`/modelos/${modeloId}/placas`),
  })
  const pendientes = (q.data?.placas ?? []).filter(
    (p) => p.modelos.some((m) => m.id === modeloId && m.estado === 'sugerido'),
  )
  const confirmar = useMutation({
    mutationFn: () =>
      api<{ confirmadas: number }>('/placas/confirmar', {
        method: 'POST',
        body: JSON.stringify({ placa_ids: pendientes.map((p) => p.id) }),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['placas-modelo', modeloId] })
      void qc.invalidateQueries({ queryKey: ['placas'] })
      void qc.invalidateQueries({ queryKey: ['placas-resumen'] })
    },
  })
  const d = q.data
  if (!d || d.total === 0) return null
  const conLaminado = d.placas.filter((p) => p.laminados.length > 0).length
  const vista = todas ? d.placas : d.placas.slice(0, PRIMERAS)

  return (
    <Card withBorder>
      <Group justify="space-between" mb="xs" align="flex-start">
        <div>
          <Text fw={500}>Placas</Text>
          <Text size="xs" c="dimmed">
            {n(d.total)} {d.total === 1 ? 'placa' : 'placas'}
            {conLaminado > 0 && ` · ${n(conLaminado)} con laminado listo`}
            {pendientes.length > 0 && ` · ${n(pendientes.length)} por confirmar`}
          </Text>
        </div>
        <Group gap="xs">
          {pendientes.length > 0 && (
            <Button size="xs" variant="light" loading={confirmar.isPending} onClick={() => confirmar.mutate()}>
              Confirmar las {n(pendientes.length)}
            </Button>
          )}
          <Button size="xs" variant="subtle" component={Link} to={`/placas?modelo_id=${modeloId}`}>
            Ver en Placas
          </Button>
        </Group>
      </Group>
      {confirmar.error && (
        <Alert color="red" mb="xs">
          {confirmar.error.message}
        </Alert>
      )}
      <SimpleGrid cols={{ base: 2, xs: 3, sm: 4, md: 6 }} spacing="xs">
        {vista.map((p) => (
          <TarjetaPlaca key={p.id} placa={p} />
        ))}
      </SimpleGrid>
      {!todas && d.placas.length > PRIMERAS && (
        <Button mt="xs" size="xs" variant="subtle" onClick={() => setTodas(true)}>
          Mostrar las {n(d.placas.length)}
        </Button>
      )}
    </Card>
  )
}
