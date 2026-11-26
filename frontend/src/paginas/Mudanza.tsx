import {
  Alert,
  Anchor,
  Badge,
  Button,
  Card,
  Code,
  Group,
  List,
  Loader,
  Modal,
  Progress,
  ScrollArea,
  SimpleGrid,
  Stack,
  Table,
  Text,
  Title,
} from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { Link, useParams } from 'react-router'

import { api } from '../api'
import { cuando, n, tamano } from '../formato'
import { useTrabajo } from '../trabajo'
import { ESTADO_MUDANZA } from '../opciones'
import type { MudanzaDetalle } from '../tipos'

export default function Mudanza() {
  const { id } = useParams()
  const qc = useQueryClient()
  const [confirmar, setConfirmar] = useState<'ejecutar' | 'deshacer' | null>(null)
  const [trabajo, setTrabajo] = useState<number | null>(null)
  const d = useQuery({ queryKey: ['mudanza', id], queryFn: () => api<MudanzaDetalle>(`/mudanzas/${id}`) })
  const t = useTrabajo(trabajo)
  const accion = useMutation({
    mutationFn: (que: 'ejecutar' | 'deshacer') =>
      api<{ trabajo_id: number }>(`/mudanzas/${id}/${que}`, { method: 'POST' }),
    onSuccess: (r) => {
      setTrabajo(r.trabajo_id)
      setConfirmar(null)
    },
  })
  const estadoTrabajo = t.data?.estado
  useEffect(() => {
    if (estadoTrabajo && estadoTrabajo !== 'en_curso') {
      for (const k of ['mudanza', 'mudanzas', 'modelos', 'modelo', 'resumen', 'historial']) {
        void qc.invalidateQueries({ queryKey: [k] })
      }
    }
  }, [estadoTrabajo, qc])

  if (d.isPending) return <Loader />
  if (d.error) return <Alert color="red">{d.error.message}</Alert>
  const m = d.data
  const p = m.plan
  const bloqueada = p.conflictos.length > 0
  const enCurso = t.data?.estado === 'en_curso' || m.estado === 'en_curso'
  const est = ESTADO_MUDANZA[m.estado]

  return (
    <Stack maw={1100}>
      <Anchor component={Link} to="/mudanzas" size="sm">
        ← Mudanzas
      </Anchor>
      <Group justify="space-between" align="flex-start">
        <div>
          <Title order={2}>Mudanza {m.id}</Title>
          <Text size="sm" c="dimmed">
            Armada {cuando(m.creada)} por {m.usuario}
            {m.ejecutada && ` · ejecutada ${cuando(m.ejecutada)}`}
            {m.deshecha && ` · deshecha ${cuando(m.deshecha)}`}
          </Text>
        </div>
        <Badge size="lg" variant="light" color={est?.color}>
          {est?.texto ?? m.estado}
        </Badge>
      </Group>

      <SimpleGrid cols={{ base: 2, sm: 4 }}>
        <Card withBorder>
          <Text size="xs" c="dimmed">MODELOS</Text>
          <Text fw={700}>{n(p.modelos.length)}</Text>
        </Card>
        <Card withBorder>
          <Text size="xs" c="dimmed">ARCHIVOS</Text>
          <Text fw={700}>{n(p.archivos)}</Text>
        </Card>
        <Card withBorder>
          <Text size="xs" c="dimmed">TAMAÑO</Text>
          <Text fw={700}>{tamano(p.bytes)}</Text>
          <Text size="xs" c="dimmed">se renombra, no se copia</Text>
        </Card>
        <Card withBorder>
          <Text size="xs" c="dimmed">SE QUEDAN</Text>
          <Text fw={700}>{n(p.se_quedan)}</Text>
          <Text size="xs" c="dimmed">sueltos en la carpeta del modelo</Text>
        </Card>
      </SimpleGrid>

      {bloqueada && (
        <Alert color="red" variant="light" title={`${p.conflictos.length} conflictos: no se puede ejecutar`}>
          <List size="sm">
            {p.conflictos.slice(0, 50).map((c) => (
              <List.Item key={c}>{c}</List.Item>
            ))}
          </List>
          <Text size="sm" mt="xs">
            Corrige los campos de esos modelos (por ejemplo, con una subcarpeta) y vuelve a armar el plan.
          </Text>
        </Alert>
      )}
      {p.avisos.length > 0 && (
        <Alert color="yellow" variant="light">
          <List size="sm">
            {p.avisos.map((a) => (
              <List.Item key={a}>{a}</List.Item>
            ))}
          </List>
        </Alert>
      )}

      {(enCurso || t.data) && (
        <Card withBorder>
          {enCurso ? (
            <Stack gap={4}>
              <Progress value={p.archivos ? ((t.data?.progreso ?? 0) * 100) / p.archivos : 100} animated />
              <Text size="sm" c="dimmed">
                {n(t.data?.progreso ?? 0)} de {n(p.archivos)} archivos…
              </Text>
            </Stack>
          ) : t.data?.estado === 'fallo' ? (
            <Alert color="red" variant="light" title="No se hizo">
              {t.data.detalle}
            </Alert>
          ) : (
            <Text size="sm">
              Listo: {n(t.data?.resultado?.movidos ?? 0)} archivos movidos,{' '}
              {n(t.data?.resultado?.carpetas_quitadas ?? 0)} carpetas vacías quitadas
              {(t.data?.resultado?.errores.length ?? 0) > 0 && `, ${t.data?.resultado?.errores.length} errores`}.
            </Text>
          )}
        </Card>
      )}

      {m.errores.length > 0 && (
        <Alert color="red" variant="light" title="Archivos con error">
          <List size="sm">
            {m.errores.map((e) => (
              <List.Item key={e.archivo}>
                {e.archivo}: {e.mensaje}
              </List.Item>
            ))}
          </List>
        </Alert>
      )}

      <Group>
        {m.estado === 'plan' && (
          <Button color="orange" disabled={bloqueada || enCurso} onClick={() => setConfirmar('ejecutar')}>
            Ejecutar mudanza
          </Button>
        )}
        {m.deshacible && (
          <Button variant="light" color="gray" disabled={enCurso} onClick={() => setConfirmar('deshacer')}>
            Deshacer esta mudanza
          </Button>
        )}
      </Group>
      {accion.error && (
        <Alert color="red" variant="light">
          {accion.error.message}
        </Alert>
      )}

      <Card withBorder>
        <Text fw={500} mb="xs">
          Modelos
        </Text>
        <ScrollArea>
          <Table miw={700}>
            <Table.Tbody>
              {p.modelos.map((x) => (
                <Table.Tr key={x.id}>
                  <Table.Td>
                    <Anchor component={Link} to={`/catalogo/${x.id}`} size="sm">
                      {x.nombre}
                    </Anchor>
                  </Table.Td>
                  <Table.Td>
                    <Text size="xs" c="dimmed" style={{ wordBreak: 'break-all' }}>
                      {x.de}
                    </Text>
                    <Text size="xs" style={{ wordBreak: 'break-all' }}>
                      → {x.a}
                    </Text>
                  </Table.Td>
                  <Table.Td ta="right">
                    <Text size="sm">
                      {n(x.archivos)} · {tamano(x.bytes)}
                    </Text>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </ScrollArea>
      </Card>

      {m.muestra.length > 0 && (
        <Card withBorder>
          <Text fw={500} mb="xs">
            Archivos {m.muestra.length < p.archivos && `(primeros ${n(m.muestra.length)} de ${n(p.archivos)})`}
          </Text>
          <ScrollArea h={320}>
            <Stack gap={6}>
              {m.muestra.map((x) => (
                <div key={x.de}>
                  <Code style={{ wordBreak: 'break-all', whiteSpace: 'normal' }}>{x.de}</Code>
                  <Text size="xs" c="dimmed" style={{ wordBreak: 'break-all' }}>
                    → {x.a}
                  </Text>
                </div>
              ))}
            </Stack>
          </ScrollArea>
        </Card>
      )}

      <Modal
        opened={confirmar !== null}
        onClose={() => setConfirmar(null)}
        title={confirmar === 'ejecutar' ? 'Ejecutar la mudanza' : 'Deshacer la mudanza'}
      >
        <Stack>
          <Text size="sm">
            {confirmar === 'ejecutar'
              ? `Se van a mover ${n(p.archivos)} archivos de ${n(p.modelos.length)} modelos en el disco. Antes se revisa cada archivo; si algo cambió desde el plan, no se mueve nada.`
              : `Los ${n(m.archivos_por_estado.movido ?? 0)} archivos movidos regresan a su carpeta anterior.`}
          </Text>
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setConfirmar(null)}>
              Cancelar
            </Button>
            <Button
              color={confirmar === 'ejecutar' ? 'orange' : 'gray'}
              loading={accion.isPending}
              onClick={() => confirmar && accion.mutate(confirmar)}
            >
              {confirmar === 'ejecutar' ? 'Mover archivos' : 'Deshacer'}
            </Button>
          </Group>
        </Stack>
      </Modal>
    </Stack>
  )
}
