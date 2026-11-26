import { ActionIcon, Alert, Button, Card, Group, Progress, SimpleGrid, Stack, Text, Tooltip, UnstyledButton } from '@mantine/core'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'

import { api } from '../api'
import { n } from '../formato'
import { useTrabajo } from '../trabajo'
import type { ArchivoFila, Ficha } from '../tipos'
import Miniatura from './Miniatura'

const PRIMERAS = 48

/** Galería de un modelo: imágenes, renders de sus mallas y vistas previas de sus escenas; desde aquí se elige la
 * portada y se abre el visor. */
export default function Galeria({ modelo, ver }: { modelo: Ficha; ver: (a: ArchivoFila) => void }) {
  const qc = useQueryClient()
  const [todas, setTodas] = useState(false)
  const [trabajo, setTrabajo] = useState<number | null>(null)
  const t = useTrabajo(trabajo)
  const generar = useMutation({
    mutationFn: () =>
      api<{ trabajo_id: number }>('/miniaturas', { method: 'POST', body: JSON.stringify({ modelo_ids: [modelo.id] }) }),
    onSuccess: (r) => setTrabajo(r.trabajo_id),
  })
  const portada = useMutation({
    mutationFn: (archivo_id: number | null) =>
      api(`/modelos/${modelo.id}/portada`, { method: 'PUT', body: JSON.stringify({ archivo_id }) }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['modelo', String(modelo.id)] })
      void qc.invalidateQueries({ queryKey: ['modelos'] })
    },
  })
  const estado = t.data?.estado
  useEffect(() => {
    if (estado && estado !== 'en_curso') void qc.invalidateQueries({ queryKey: ['modelo', String(modelo.id)] })
  }, [estado, qc, modelo.id])

  const presentes = modelo.archivos.filter((a) => a.estado === 'presente')
  const sin = presentes.filter((a) => a.renderizable && !a.miniatura).length
  const con = presentes
    .filter((a) => a.miniatura)
    .sort((a, b) => Number(b.tipo === 'imagen') - Number(a.tipo === 'imagen') || b.bytes - a.bytes)
  const vista = todas ? con : con.slice(0, PRIMERAS)
  const enCurso = t.data?.estado === 'en_curso'

  return (
    <Card withBorder>
      <Group justify="space-between" mb="xs">
        <div>
          <Text fw={500}>Galería</Text>
          <Text size="xs" c="dimmed">
            {n(con.length)} con miniatura{sin > 0 && ` · ${n(sin)} sin miniatura`} · la estrella elige la portada
          </Text>
        </div>
        {sin > 0 && (
          <Button size="xs" variant="light" loading={generar.isPending || enCurso} onClick={() => generar.mutate()}>
            Generar las que faltan
          </Button>
        )}
      </Group>
      {generar.error && (
        <Alert color="red" variant="light" mb="xs">
          {generar.error.message}
        </Alert>
      )}
      {enCurso && (
        <Stack gap={2} mb="xs">
          <Progress value={sin ? ((t.data?.progreso ?? 0) * 100) / sin : 100} animated />
          <Text size="xs" c="dimmed">
            {n(t.data?.progreso ?? 0)} de {n(sin)}…
          </Text>
        </Stack>
      )}
      {t.data?.estado === 'terminado' && (t.data.resultado?.fallidas ?? 0) > 0 && (
        <Alert color="yellow" variant="light" mb="xs">
          {n(t.data.resultado?.fallidas ?? 0)} no se pudieron generar
          {t.data.resultado?.ejemplos_de_error?.[0] && `: ${t.data.resultado.ejemplos_de_error[0]}`}
        </Alert>
      )}
      {con.length === 0 ? (
        <Text size="sm" c="dimmed">
          Todavía no hay miniaturas.
        </Text>
      ) : (
        <SimpleGrid cols={{ base: 3, xs: 4, sm: 5, md: 6 }} spacing="xs">
          {vista.map((a) => {
            const esPortada = modelo.portada?.id === a.id
            return (
              <Stack key={a.id} gap={2} style={{ position: 'relative' }}>
                <UnstyledButton onClick={() => ver(a)} title={a.nombre}>
                  <Miniatura archivoId={a.id} clave={a.miniatura} lado={120} />
                </UnstyledButton>
                <Text size="xs" c="dimmed" lineClamp={1} title={a.nombre}>
                  {a.nombre}
                </Text>
                <Tooltip label={esPortada ? (modelo.portada_elegida ? 'Portada elegida (quitar)' : 'Portada automática') : 'Usar como portada'}>
                  <ActionIcon
                    size="sm"
                    variant={esPortada ? 'filled' : 'default'}
                    color="yellow"
                    style={{ position: 'absolute', top: 4, right: 4 }}
                    aria-label="Portada"
                    loading={portada.isPending && portada.variables === a.id}
                    onClick={() => portada.mutate(esPortada && modelo.portada_elegida === a.id ? null : a.id)}
                  >
                    ★
                  </ActionIcon>
                </Tooltip>
              </Stack>
            )
          })}
        </SimpleGrid>
      )}
      {!todas && con.length > PRIMERAS && (
        <Button mt="xs" size="xs" variant="subtle" onClick={() => setTodas(true)}>
          Mostrar las {n(con.length)}
        </Button>
      )}
    </Card>
  )
}
